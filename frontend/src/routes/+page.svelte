<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.svelte';
	import type { Station, NowPlaying } from '$lib/types';
	import Hls from 'hls.js';
	import DancingAnimals from '$lib/components/DancingAnimals.svelte';

	let stations = $state<Station[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let selectedIndex = $state(0);
	let nowPlaying = $state<NowPlaying | null>(null);
	let isPlaying = $state(false);
	let audioElement = $state<HTMLAudioElement | undefined>();
	let hls: Hls | null = null;
	let pollInterval: number;
	let currentPosition = $state(0);
	let progressInterval: number;
	let sessionId = $state<string>('');
	let heartbeatInterval: number;
	let listenerCounts = $state<Record<string, number>>({});
	let listenerCountsInterval: number;
	let playbackAbortController: AbortController | null = null;
	let mobileView = $state<'stations' | 'visualizer'>('stations');
	let siteTitle = $state('NAVIDROME RADIO');

	let selectedStation = $derived(stations[selectedIndex] || null);

	// Trigger resize when switching to visualizer view (Three.js needs this)
	$effect(() => {
		if (mobileView === 'visualizer') {
			// Small delay to let the DOM update before triggering resize
			setTimeout(() => {
				window.dispatchEvent(new Event('resize'));
			}, 50);
		}
	});

	onMount(async () => {
		sessionId = crypto.randomUUID();

		// Load site title
		try {
			const settings = await api.getSettings();
			siteTitle = settings.site_title;
		} catch (e) {
			console.error('Failed to load settings:', e);
		}

		try {
			stations = await api.getStations();
			// Filter to only active stations
			stations = stations.filter(s => s.active);
			if (stations.length > 0) {
				await updateNowPlaying();
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load stations';
		} finally {
			loading = false;
		}

		// Poll for now playing updates
		pollInterval = setInterval(updateNowPlaying, 3000);
		progressInterval = setInterval(updateProgress, 500);

		// Poll listener counts
		updateListenerCounts();
		listenerCountsInterval = setInterval(updateListenerCounts, 10000);
	});

	onDestroy(() => {
		if (pollInterval) clearInterval(pollInterval);
		if (progressInterval) clearInterval(progressInterval);
		if (heartbeatInterval) clearInterval(heartbeatInterval);
		if (listenerCountsInterval) clearInterval(listenerCountsInterval);
		stopPlayback();
	});

	async function updateNowPlaying() {
		if (!selectedStation) return;
		try {
			nowPlaying = await api.getNowPlaying(selectedStation.id);
		} catch (e) {
			console.error('Failed to get now playing:', e);
		}
	}

	async function updateListenerCounts() {
		try {
			const result = await api.getListenerCounts();
			listenerCounts = result.counts;
		} catch (e) {
			console.error('Failed to get listener counts:', e);
		}
	}

	function updateProgress() {
		if (!nowPlaying) return;
		const startedAt = new Date(nowPlaying.started_at).getTime();
		const now = Date.now();
		const elapsed = (now - startedAt) / 1000;
		currentPosition = Math.min(elapsed, nowPlaying.track.duration);
	}

	async function selectStation(index: number) {
		if (index === selectedIndex) return;
		const wasPlaying = isPlaying;

		// Stop current playback first and ensure clean state
		stopPlayback();

		selectedIndex = index;
		nowPlaying = null;

		// Fetch now playing for the new station
		await updateNowPlaying();

		if (wasPlaying) {
			// Delay to ensure audio element is fully reset before starting new stream
			await new Promise(r => setTimeout(r, 250));
			startPlayback();
		}
	}

	function tuneUp() {
		if (selectedIndex > 0) {
			selectStation(selectedIndex - 1);
		}
	}

	function tuneDown() {
		if (selectedIndex < stations.length - 1) {
			selectStation(selectedIndex + 1);
		}
	}

	async function startPlayback() {
		if (!audioElement || !selectedStation) return;

		// Abort any pending playback
		if (playbackAbortController) {
			playbackAbortController.abort();
		}
		playbackAbortController = new AbortController();
		const signal = playbackAbortController.signal;

		isPlaying = true;
		// Add cache-busting parameter to force fresh playlist
		const streamUrl = `/api/v1/stations/${selectedStation.id}/stream/playlist.m3u8?_t=${Date.now()}`;

		if (Hls.isSupported()) {
			hls = new Hls({
				enableWorker: true,
				lowLatencyMode: true,
				liveSyncDurationCount: 2,       // Fewer segments to buffer for live
				liveMaxLatencyDurationCount: 4, // Lower max latency
				liveDurationInfinity: true,     // Treat as live stream
				backBufferLength: 0,            // Don't keep back buffer - live stream
				maxBufferLength: 6,             // Only buffer 6 seconds ahead
				maxMaxBufferLength: 10,         // Hard limit on buffer
			});

			// Handle HLS errors with retry logic
			let errorRetries = 0;
			const MAX_RETRIES = 3;

			hls.on(Hls.Events.ERROR, (event, data) => {
				console.warn('HLS error:', data.type, data.details, data.fatal ? '(fatal)' : '');

				if (data.fatal) {
					errorRetries++;

					if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
						// For levelEmptyError (no segments), wait and retry
						if (data.details === 'levelEmptyError') {
							if (errorRetries <= MAX_RETRIES) {
								console.log(`Playlist empty, waiting for segments (attempt ${errorRetries}/${MAX_RETRIES})...`);
								setTimeout(() => {
									hls?.startLoad();
								}, 1000); // Wait 1 second for segments to be ready
							} else {
								console.log('Max retries for empty playlist, restarting...');
								stopPlayback();
								setTimeout(() => startPlayback(), 1000);
							}
						} else {
							// Other network errors
							console.log('Attempting network error recovery...');
							hls?.startLoad();
						}
					} else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
						// For media errors (including bufferAppendError), try recovery
						if (errorRetries <= MAX_RETRIES) {
							console.log(`Attempting media error recovery (attempt ${errorRetries}/${MAX_RETRIES})...`);
							hls?.recoverMediaError();
						} else {
							// Too many retries, restart the stream
							console.log('Max media error retries reached, restarting stream...');
							stopPlayback();
							setTimeout(() => startPlayback(), 1000);
						}
					} else {
						// Can't recover, restart playback
						console.log('Unrecoverable error, restarting stream...');
						stopPlayback();
						setTimeout(() => startPlayback(), 1000);
					}
				} else {
					// Non-fatal error, reset retry counter on success
					errorRetries = 0;
				}
			});

			hls.loadSource(streamUrl);
			hls.attachMedia(audioElement);
			hls.on(Hls.Events.MANIFEST_PARSED, async () => {
				if (signal.aborted) return;
				try {
					await audioElement?.play();
				} catch (e) {
					// Ignore AbortError - expected when switching stations
					if (e instanceof Error && e.name !== 'AbortError') {
						console.error('Playback error:', e);
					}
				}
			});
		} else if (audioElement.canPlayType('application/vnd.apple.mpegurl')) {
			audioElement.src = streamUrl;
			try {
				await audioElement.play();
			} catch (e) {
				if (e instanceof Error && e.name !== 'AbortError') {
					console.error('Playback error:', e);
				}
			}
		}

		startHeartbeat();
	}

	function stopPlayback() {
		// Abort any pending playback first
		if (playbackAbortController) {
			playbackAbortController.abort();
			playbackAbortController = null;
		}

		isPlaying = false;

		// Destroy HLS instance first (this detaches from audio element)
		if (hls) {
			hls.stopLoad();  // Stop loading new segments
			hls.destroy();
			hls = null;
		}

		// Reset audio element completely
		if (audioElement) {
			audioElement.pause();
			audioElement.removeAttribute('src');
			audioElement.load();  // Reset the element's internal state
		}

		if (heartbeatInterval) {
			clearInterval(heartbeatInterval);
			heartbeatInterval = 0;
		}
	}

	function togglePlayback() {
		if (isPlaying) {
			stopPlayback();
		} else {
			startPlayback();
		}
	}

	function startHeartbeat() {
		if (!selectedStation || heartbeatInterval) return;
		sendHeartbeat();
		heartbeatInterval = setInterval(sendHeartbeat, 10000);
	}

	async function sendHeartbeat() {
		if (!selectedStation || !sessionId) return;
		try {
			await api.listenerHeartbeat(selectedStation.id, sessionId);
		} catch (e) {
			console.error('Heartbeat failed:', e);
		}
	}

	async function handleSkip() {
		if (!selectedStation || !authStore.isAdmin) return;
		try {
			await api.skipTrack(selectedStation.id);
			// Reload stream
			if (isPlaying) {
				stopPlayback();
				await new Promise(r => setTimeout(r, 300));
				startPlayback();
			}
			await updateNowPlaying();
		} catch (e) {
			console.error('Skip failed:', e);
		}
	}

	function formatTime(seconds: number): string {
		const mins = Math.floor(seconds / 60);
		const secs = Math.floor(seconds % 60);
		return `${mins}:${secs.toString().padStart(2, '0')}`;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowUp' || e.key === 'k') {
			e.preventDefault();
			tuneUp();
		} else if (e.key === 'ArrowDown' || e.key === 'j') {
			e.preventDefault();
			tuneDown();
		} else if (e.key === ' ' || e.key === 'Enter') {
			e.preventDefault();
			togglePlayback();
		} else if (e.key === 'n' && authStore.isAdmin) {
			e.preventDefault();
			handleSkip();
		}
	}
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="tuner-container">
	<audio bind:this={audioElement} crossorigin="anonymous"></audio>

	{#if loading}
		<div class="loading">
			<pre class="blink">LOADING...</pre>
		</div>
	{:else if error}
		<div class="error-box">
			<pre>┌─ ERROR ─────────────────────┐
│ {error.padEnd(27)} │
└─────────────────────────────┘</pre>
		</div>
	{:else}
		<!-- Header -->
		<header class="header">
			<div class="header-content">
				<span class="header-left">┌──</span>
				<h1 class="title">{siteTitle}</h1>
				<span class="header-right">──┐</span>
			</div>
			<div class="header-sub">
				<span class="sub-left">│</span>
				<span class="subtitle">AI-Curated Music Streams</span>
				<span class="sub-right">│</span>
			</div>
			<div class="header-auth">
				<span class="auth-border">├</span>
				{#if authStore.isAuthenticated}
					<span class="auth-user">● {authStore.user?.username}</span>
					{#if authStore.isAdmin}
						<a href="/admin" class="auth-link">[ADMIN]</a>
					{/if}
					<button class="auth-btn" onclick={() => authStore.logout()}>[LOGOUT]</button>
				{:else}
					<a href="/login" class="auth-link">[LOGIN]</a>
				{/if}
				<span class="auth-border">┤</span>
			</div>
		</header>

		<main class="main-content">
			<!-- Left: Station Selector -->
			<section class="station-selector" class:mobile-hidden={mobileView !== 'stations'}>
				<div class="section-header">
					<span class="corner">┌</span>
					<span class="line"></span>
					<span class="title desktop-only">STATIONS</span>
					<span class="title mobile-only">
						<button class="inline-toggle" class:active={mobileView === 'stations'} onclick={() => mobileView = 'stations'}>STATIONS</button>
						<button class="inline-toggle" class:active={mobileView === 'visualizer'} onclick={() => mobileView = 'visualizer'}>VISUALIZER</button>
					</span>
					<span class="line"></span>
					<span class="corner">┐</span>
				</div>
				<!-- Desktop: vertical list -->
				<div class="section-body desktop-only">
					<button class="tune-arrow tune-up" onclick={tuneUp} disabled={selectedIndex === 0}>
						<span>▲</span>
					</button>
					<div class="station-list">
						{#each stations as station, i}
							<button
								class="station-item"
								class:selected={i === selectedIndex}
								onclick={() => selectStation(i)}
							>
								<span class="selector">{i === selectedIndex ? '►' : '○'}</span>
								<span class="freq">{(88.1 + i * 0.2).toFixed(1)}</span>
								<span class="name">{station.name}</span>
								<span class="listeners">[{listenerCounts[station.id] || 0}]</span>
							</button>
						{/each}
					</div>
					<button class="tune-arrow tune-down" onclick={tuneDown} disabled={selectedIndex === stations.length - 1}>
						<span>▼</span>
					</button>
				</div>
				<!-- Mobile: horizontal carousel with centered active station -->
				<div class="section-body mobile-only mobile-station-carousel">
					<button class="carousel-arrow" onclick={tuneUp} disabled={selectedIndex === 0}>
						<span>◄</span>
					</button>
					<div class="carousel-station">
						{#if selectedStation}
							<span class="carousel-freq">{(88.1 + selectedIndex * 0.2).toFixed(1)}</span>
							<span class="carousel-name">{selectedStation.name}</span>
							<span class="carousel-listeners">[{listenerCounts[selectedStation.id] || 0}]</span>
						{/if}
					</div>
					<button class="carousel-arrow" onclick={tuneDown} disabled={selectedIndex === stations.length - 1}>
						<span>►</span>
					</button>
				</div>
				<div class="section-footer">
					<span class="corner">└</span>
					<span class="line"></span>
					<span class="title desktop-only">↑/↓ to tune</span>
					<span class="title mobile-only">◄/► to tune</span>
					<span class="line"></span>
					<span class="corner">┘</span>
				</div>
			</section>

			<!-- Center: Selected Station Details -->
			<section class="station-details" class:mobile-hidden={mobileView !== 'visualizer'}>
				{#if selectedStation}
					<div class="section-header">
						<span class="corner">┌</span>
						<span class="line"></span>
						<span class="title desktop-only">{selectedStation.name.toUpperCase()}</span>
						<span class="title mobile-only">
							<button class="inline-toggle" class:active={mobileView === 'stations'} onclick={() => mobileView = 'stations'}>STATIONS</button>
							<button class="inline-toggle" class:active={mobileView === 'visualizer'} onclick={() => mobileView = 'visualizer'}>VISUALIZER</button>
						</span>
						<span class="line"></span>
						<span class="corner">┐</span>
					</div>
					<div class="section-body details-content">
						<div class="station-image">
							<pre class="placeholder-art">╭────────────╮
│  ♪  ♫  ♪  │
│   ◉    ◉  │
│     ◡     │
╰────────────╯</pre>
						</div>
						<div class="station-info">
							<p class="description">{selectedStation.description}</p>
							<div class="genres">
								<span class="label">GENRES:</span>
								{#each selectedStation.genres.slice(0, 4) as genre}
									<span class="tag">[{genre}]</span>
								{/each}
							</div>
							<div class="status-line">
								<span class="status" class:live={selectedStation.active}>
									{selectedStation.active ? '● LIVE' : '○ OFFLINE'}
								</span>
								<span class="listeners-count">
									{listenerCounts[selectedStation.id] || 0} listening
								</span>
							</div>
						</div>
						<!-- 3D Dancing Animals Visualization -->
						<div class="viz-container">
							<DancingAnimals
								{audioElement}
								{isPlaying}
							/>
						</div>
					</div>
					<div class="section-footer">
						<span class="corner">└</span>
						<span class="line"></span>
						<span class="corner">┘</span>
					</div>
				{/if}
			</section>

			<!-- Right: Now Playing -->
			<section class="now-playing">
				<div class="section-header">
					<span class="corner">┌</span>
					<span class="line"></span>
					<span class="title">NOW PLAYING</span>
					<span class="line"></span>
					<span class="corner">┐</span>
				</div>
				{#if nowPlaying}
					<div class="section-body np-content">
						<div class="album-art">
							{#if nowPlaying.track.albumArt}
								<img src={nowPlaying.track.albumArt} alt={nowPlaying.track.album} />
							{:else}
								<div class="no-art">
									<pre>{`
  ╔════════════╗
  ║            ║
  ║   NO ART   ║
  ║            ║
  ╚════════════╝
									`.trim()}</pre>
								</div>
							{/if}
						</div>
						<div class="track-info">
							<p class="title">{nowPlaying.track.title}</p>
							<p class="artist">{nowPlaying.track.artist}</p>
							<p class="album">{nowPlaying.track.album}</p>
						</div>
						<div class="progress">
							<div class="progress-bar">
								<span class="elapsed">{formatTime(currentPosition)}</span>
								<div class="bar">
									<div class="fill" style="width: {(currentPosition / nowPlaying.track.duration) * 100}%"></div>
								</div>
								<span class="duration">{formatTime(nowPlaying.track.duration)}</span>
							</div>
						</div>
					</div>
				{:else}
					<div class="section-body np-empty">
						<pre>  No track info available</pre>
					</div>
				{/if}
				<div class="section-footer">
					<span class="corner">└</span>
					<span class="line"></span>
					<span class="corner">┘</span>
				</div>
			</section>
		</main>

		<!-- Controls -->
		<footer class="controls">
			<div class="control-bar">
				<span class="corner">├</span>
				<span class="line"></span>
				<button class="ctrl-btn" onclick={togglePlayback}>
					{isPlaying ? 'STOP' : 'PLAY'}
				</button>
				{#if authStore.isAdmin}
					<button class="ctrl-btn" onclick={handleSkip}>
						SKIP
					</button>
				{/if}
				<span class="line"></span>
				<span class="help">↑↓:tune  SPACE:play  {authStore.isAdmin ? 'n:skip' : ''}</span>
				<span class="line"></span>
				<span class="corner">┤</span>
			</div>
		</footer>
	{/if}
</div>

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		height: 100%;
		overflow: hidden;
		background: #0a0a0a;
		color: #e0e0e0;
		font-family: 'Berkeley Mono', 'JetBrains Mono', 'Fira Code', 'SF Mono', monospace;
	}

	.tuner-container {
		height: 100vh;
		display: flex;
		flex-direction: column;
		padding: 0.75rem 1rem;
		box-sizing: border-box;
		overflow: hidden;
	}

	.loading, .error-box {
		display: flex;
		align-items: center;
		justify-content: center;
		flex: 1;
	}

	.blink {
		animation: blink 1s infinite;
	}

	@keyframes blink {
		50% { opacity: 0.5; }
	}

	/* Header */
	.header {
		text-align: center;
		flex-shrink: 0;
		border-bottom: 1px solid #333;
		padding-bottom: 0.5rem;
		margin-bottom: 0.75rem;
	}

	.header-content {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
	}

	.header-left, .header-right {
		color: #333;
		flex: 1;
		max-width: 200px;
	}

	.header-left {
		text-align: right;
	}

	.header-right {
		text-align: left;
	}

	.title {
		font-size: 1.5rem;
		font-weight: bold;
		color: #00ff88;
		letter-spacing: 0.2em;
		margin: 0;
		text-shadow: 0 0 20px rgba(0, 255, 136, 0.3);
	}

	.header-sub {
		display: flex;
		align-items: center;
		justify-content: center;
		margin-top: 0.25rem;
	}

	.sub-left, .sub-right {
		color: #333;
		flex: 1;
		max-width: 200px;
	}

	.sub-left {
		text-align: right;
		padding-right: 1rem;
	}

	.sub-right {
		text-align: left;
		padding-left: 1rem;
	}

	.subtitle {
		color: #666;
		font-size: 0.75rem;
		letter-spacing: 0.15em;
		text-transform: uppercase;
	}

	.header-auth {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		margin-top: 0.25rem;
		font-size: 0.75rem;
	}

	.auth-border {
		color: #333;
	}

	.auth-user {
		color: #00ff88;
	}

	.auth-link {
		color: #888;
		text-decoration: none;
		transition: color 0.15s;
	}

	.auth-link:hover {
		color: #00ff88;
	}

	.auth-btn {
		background: transparent;
		border: none;
		color: #666;
		font-family: inherit;
		font-size: 0.75rem;
		cursor: pointer;
		padding: 0;
		transition: color 0.15s;
	}

	.auth-btn:hover {
		color: #ff6b6b;
	}

	.main-content {
		display: grid;
		grid-template-columns: 220px 1fr 320px;
		gap: 1rem;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	/* Mobile/Desktop visibility */
	.mobile-only {
		display: none;
	}

	.inline-toggle {
		background: transparent;
		border: none;
		color: #555;
		font-family: inherit;
		font-size: 0.75rem;
		cursor: pointer;
		padding: 0;
		transition: color 0.15s;
	}

	.inline-toggle:hover {
		color: #888;
	}

	.inline-toggle.active {
		color: #00ff88;
	}

	@media (max-width: 1000px) {
		.tuner-container {
			padding: 0.5rem 0.75rem;
		}

		.header {
			padding-bottom: 0.25rem;
			margin-bottom: 0.5rem;
		}

		.header .title {
			font-size: 1.25rem;
			letter-spacing: 0.15em;
		}

		.desktop-only {
			display: none !important;
		}

		.mobile-only {
			display: flex;
			gap: 0.75rem;
		}

		.main-content {
			grid-template-columns: 1fr;
			grid-template-rows: auto 1fr;
			gap: 0.5rem;
		}

		.mobile-hidden {
			display: none !important;
		}

		/* Station selector carousel on mobile */
		.station-selector .mobile-station-carousel {
			display: flex;
		}

		/* Station details/visualizer on mobile */
		.station-details {
			flex: 1;
			min-height: 150px;
		}

		.station-details .details-content {
			padding: 0.5rem;
			gap: 0.5rem;
		}

		.station-details .viz-container {
			flex: 1;
			min-height: 120px;
			height: 100%;
		}

		/* Hide station info on mobile visualizer view to give more space */
		.station-details .station-image,
		.station-details .station-info {
			display: none;
		}

		/* Now playing on mobile */
		.now-playing .np-content {
			padding: 0.5rem;
			gap: 0.4rem;
		}

		.album-art img {
			max-width: 200px;
			margin: 0 auto;
			display: block;
		}

		.track-info .title {
			font-size: 0.85rem;
		}

		.track-info .artist {
			font-size: 0.75rem;
		}

		.track-info .album {
			font-size: 0.7rem;
		}

		/* Controls */
		.controls {
			margin-top: 0.25rem;
		}

		.control-bar {
			padding: 0.3rem 0;
		}

		.ctrl-btn {
			font-size: 0.75rem;
			padding: 0.15rem 0.5rem;
		}

		.help {
			font-size: 0.6rem;
		}
	}

	.section-header, .section-footer {
		display: flex;
		align-items: center;
		font-size: 0.7rem;
		color: #333;
		flex-shrink: 0;
		padding: 0 1px; /* Align corners with 1px border */
	}

	.section-header .corner,
	.section-footer .corner {
		flex-shrink: 0;
		line-height: 1;
	}

	.section-header .line,
	.section-footer .line {
		flex: 1;
		height: 1px;
		background: #333;
		min-width: 4px;
	}

	.section-header .title,
	.section-footer .title {
		padding: 0 0.4rem;
		color: #555;
		white-space: nowrap;
		font-size: 0.65rem;
		letter-spacing: 0.05em;
	}

	.section-body {
		flex: 1;
		border-left: 1px solid #333;
		border-right: 1px solid #333;
		min-height: 0;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	/* Station Selector */
	.station-selector {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
	}

	/* Mobile carousel - hidden on desktop */
	.mobile-station-carousel {
		display: none;
		flex-direction: row;
		align-items: center;
		justify-content: center;
		padding: 0.5rem;
		gap: 0.5rem;
	}

	.carousel-arrow {
		background: transparent;
		border: 1px solid #333;
		color: #00ff88;
		font-family: inherit;
		font-size: 1.2rem;
		padding: 0.5rem 0.75rem;
		cursor: pointer;
		transition: all 0.15s;
		flex-shrink: 0;
	}

	.carousel-arrow:hover:not(:disabled) {
		background: #1a2a1a;
		border-color: #00ff88;
	}

	.carousel-arrow:disabled {
		color: #333;
		border-color: #222;
		cursor: not-allowed;
	}

	.carousel-station {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		text-align: center;
		padding: 0.5rem;
		background: linear-gradient(180deg, #0a1a0a 0%, #1a2a1a 50%, #0a1a0a 100%);
		border: 1px solid #00ff88;
		min-width: 0;
	}

	.carousel-freq {
		color: #ff8800;
		font-size: 1.1rem;
		font-weight: bold;
	}

	.carousel-name {
		color: #00ff88;
		font-size: 0.9rem;
		font-weight: bold;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 100%;
	}

	.carousel-listeners {
		color: #666;
		font-size: 0.7rem;
	}

	.tune-arrow {
		display: flex;
		align-items: center;
		justify-content: center;
		background: #111;
		border: none;
		border-bottom: 1px solid #333;
		color: #00ff88;
		font-family: inherit;
		font-size: 0.9rem;
		padding: 0.3rem;
		cursor: pointer;
		transition: all 0.1s;
	}

	.tune-arrow.tune-down {
		border-bottom: none;
		border-top: 1px solid #333;
	}

	.tune-arrow:hover:not(:disabled) {
		background: #1a2a1a;
		color: #00ff88;
	}

	.tune-arrow:disabled {
		color: #333;
		cursor: not-allowed;
	}

	.station-list {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		min-height: 0;
	}

	.station-list::-webkit-scrollbar {
		width: 4px;
	}

	.station-list::-webkit-scrollbar-track {
		background: #1a1a1a;
	}

	.station-list::-webkit-scrollbar-thumb {
		background: #333;
	}

	.station-item {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.35rem 0.5rem;
		width: 100%;
		background: transparent;
		border: none;
		color: #555;
		font-family: inherit;
		font-size: 0.7rem;
		cursor: pointer;
		text-align: left;
		transition: all 0.15s ease;
	}

	.station-item:hover {
		background: #151515;
		color: #888;
	}

	.station-item.selected {
		background: linear-gradient(90deg, #0a1a0a 0%, #1a2a1a 50%, #0a1a0a 100%);
		color: #00ff88;
		font-size: 0.85rem;
		padding: 0.6rem 0.5rem;
		border-left: 2px solid #00ff88;
		margin-left: -1px;
	}

	.station-item .selector {
		width: 1rem;
		color: #333;
		font-size: 0.6rem;
	}

	.station-item.selected .selector {
		color: #00ff88;
		font-size: 0.85rem;
	}

	.station-item .freq {
		color: #664400;
		width: 2.5rem;
		font-size: 0.65rem;
	}

	.station-item.selected .freq {
		color: #ff8800;
		font-size: 0.8rem;
		width: 2.8rem;
	}

	.station-item .name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.station-item .listeners {
		color: #444;
		font-size: 0.6rem;
	}

	.station-item.selected .listeners {
		color: #666;
		font-size: 0.7rem;
	}

	/* Station Details */
	.station-details {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
	}

	.details-content {
		flex: 1;
		padding: 0.75rem 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		min-height: 0;
	}

	.station-image {
		text-align: center;
		flex-shrink: 0;
	}

	.placeholder-art {
		color: #444;
		font-size: 0.75rem;
		line-height: 1.1;
		margin: 0;
		display: inline-block;
		border: 1px solid #333;
		padding: 0.5rem 0.75rem;
		background: #0a0a0a;
	}

	.station-info {
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.description {
		color: #888;
		margin-bottom: 0.75rem;
		line-height: 1.4;
		font-size: 0.85rem;
		overflow: hidden;
		text-overflow: ellipsis;
		display: -webkit-box;
		-webkit-line-clamp: 3;
		-webkit-box-orient: vertical;
	}

	.genres {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		align-items: center;
		margin-bottom: 1rem;
	}

	.genres .label {
		color: #666;
		font-size: 0.8rem;
	}

	.genres .tag {
		color: #888;
		font-size: 0.8rem;
	}

	.status-line {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding-top: 0.5rem;
		border-top: 1px dashed #333;
	}

	.status {
		color: #666;
	}

	.status.live {
		color: #00ff88;
	}

	.listeners-count {
		color: #666;
		font-size: 0.85rem;
	}

	/* Visualization */
	.viz-container {
		flex: 1;
		display: flex;
		border: 1px solid #2a2a2a;
		min-height: 280px;
		overflow: hidden;
	}

	/* Now Playing */
	.now-playing {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
	}

	.np-content {
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.album-art {
		flex-shrink: 0;
		width: 100%;
	}

	.album-art img {
		width: 100%;
		aspect-ratio: 1;
		object-fit: cover;
		border: 2px solid #333;
	}

	.no-art {
		color: #333;
		font-size: 0.6rem;
		text-align: center;
	}

	.track-info {
		text-align: center;
		flex-shrink: 0;
	}

	.track-info .title {
		color: #fff;
		font-size: 0.9rem;
		font-weight: bold;
		margin-bottom: 0.2rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.track-info .artist {
		color: #00ff88;
		font-size: 0.8rem;
		margin-bottom: 0.2rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.track-info .album {
		color: #555;
		font-size: 0.75rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.progress {
		margin-top: auto;
		flex-shrink: 0;
	}

	.progress-bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.progress-bar .elapsed,
	.progress-bar .duration {
		font-size: 0.7rem;
		color: #555;
		width: 2.5rem;
	}

	.progress-bar .elapsed {
		text-align: right;
	}

	.progress-bar .bar {
		flex: 1;
		height: 3px;
		background: #333;
		position: relative;
	}

	.progress-bar .fill {
		position: absolute;
		left: 0;
		top: 0;
		height: 100%;
		background: #00ff88;
		transition: width 0.5s linear;
	}

	.np-empty {
		display: flex;
		align-items: center;
		justify-content: center;
		color: #333;
		font-size: 0.8rem;
		padding: 2rem;
	}

	/* Controls */
	.controls {
		margin-top: 0.5rem;
		flex-shrink: 0;
	}

	.control-bar {
		display: flex;
		align-items: center;
		padding: 0.4rem 0;
	}

	.control-bar .corner {
		color: #333;
		flex-shrink: 0;
	}

	.control-bar .line {
		flex: 1;
		height: 1px;
		background: #333;
		min-width: 8px;
	}

	.ctrl-btn {
		background: transparent;
		border: 1px solid #333;
		color: #00ff88;
		font-family: inherit;
		font-size: 0.8rem;
		cursor: pointer;
		padding: 0.2rem 0.6rem;
		margin: 0 0.25rem;
		transition: all 0.1s;
	}

	.ctrl-btn:hover {
		background: #1a2a1a;
	}

	.help {
		color: #444;
		font-size: 0.7rem;
		padding: 0 0.5rem;
	}
</style>
