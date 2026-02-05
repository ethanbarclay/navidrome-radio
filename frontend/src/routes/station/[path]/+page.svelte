<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.svelte';
	import DancingAnimals from '$lib/components/DancingAnimals.svelte';
	import Hls from 'hls.js';
	import type { NowPlaying, Station } from '$lib/types';

	// Prevent scrolling on mount
	onMount(() => {
		document.documentElement.style.setProperty('overflow', 'hidden', 'important');
		document.body.style.setProperty('overflow', 'hidden', 'important');
		document.documentElement.style.height = '100%';
		document.body.style.height = '100%';

		return () => {
			document.documentElement.style.removeProperty('overflow');
			document.body.style.removeProperty('overflow');
			document.documentElement.style.height = '';
			document.body.style.height = '';
		};
	});

	let stationPath = $derived($page.params.path);
	let station = $state<Station | null>(null);
	let nowPlaying = $state<NowPlaying | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let audioElement = $state<HTMLAudioElement | undefined>();
	let isMuted = $state(
		typeof localStorage !== 'undefined' ? localStorage.getItem('radio_muted') === 'true' : false
	);
	let pollInterval: number;
	let currentPosition = $state(0);
	let progressInterval: number;
	let hasStartedPlaying = $state(false);
	let mediaSession: MediaSession | null = null;

	// HLS state
	let hls: Hls | null = null;
	let hlsError = $state<string | null>(null);
	let isBuffering = $state(false);

	// 3D visualizer is always shown when playing (no toggle needed)

	// Listener tracking
	let sessionId = $state<string>('');
	let heartbeatInterval: number;

	// Audio event listener state (to avoid accumulating listeners)
	let audioListenersAttached = false;

	// Handle page unload
	function handleBeforeUnload() {
		if (station && sessionId && hasStartedPlaying) {
			const data = JSON.stringify({ session_id: sessionId });
			navigator.sendBeacon(`/api/v1/stations/${station.id}/listener/leave`, data);
		}
	}

	onMount(async () => {
		sessionId = crypto.randomUUID();
		window.addEventListener('beforeunload', handleBeforeUnload);

		if ('mediaSession' in navigator) {
			mediaSession = navigator.mediaSession;
			setupMediaSession();
		}

		try {
			const stations = await api.getStations();
			station = stations.find((s) => s.path === stationPath) || null;

			if (!station) {
				error = 'Station not found';
				loading = false;
				return;
			}

			if (!station.active) {
				error = 'This station is not currently broadcasting';
				loading = false;
				return;
			}

			// Get initial now playing info
			await updateNowPlaying();

			// Poll for track info updates
			pollInterval = setInterval(updateNowPlaying, 3000);

			// Update progress based on now playing
			progressInterval = setInterval(updateProgress, 500);

			loading = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load station';
			loading = false;
		}
	});

	onDestroy(() => {
		if (pollInterval) clearInterval(pollInterval);
		if (progressInterval) clearInterval(progressInterval);
		if (heartbeatInterval) clearInterval(heartbeatInterval);
		if (hls) {
			hls.destroy();
			hls = null;
		}
		window.removeEventListener('beforeunload', handleBeforeUnload);
		if (station && sessionId && hasStartedPlaying) {
			const data = JSON.stringify({ session_id: sessionId });
			navigator.sendBeacon(`/api/v1/stations/${station.id}/listener/leave`, data);
		}
	});

	function initHls() {
		if (!audioElement || !station) return;

		const streamUrl = `/api/v1/stations/${station.id}/stream/playlist.m3u8`;

		if (Hls.isSupported()) {
			hls = new Hls({
				enableWorker: true,
				lowLatencyMode: false,  // Not LL-HLS, standard 2s segments
				backBufferLength: 10,   // Keep less back buffer for live
				maxBufferLength: 30,    // Max forward buffer
				liveSyncDuration: 4,    // Target 2 segments behind live edge
				liveMaxLatencyDuration: 10,
				liveDurationInfinity: true,  // Treat as live stream
			});

			hls.loadSource(streamUrl);
			hls.attachMedia(audioElement);

			hls.on(Hls.Events.MANIFEST_PARSED, () => {
				console.log('HLS manifest loaded');
				if (hasStartedPlaying) {
					audioElement?.play();
				}
			});

			hls.on(Hls.Events.ERROR, (event, data) => {
				console.error('HLS error:', data);
				if (data.fatal) {
					switch (data.type) {
						case Hls.ErrorTypes.NETWORK_ERROR:
							hlsError = 'Network error - retrying...';
							hls?.startLoad();
							break;
						case Hls.ErrorTypes.MEDIA_ERROR:
							hlsError = 'Media error - recovering...';
							hls?.recoverMediaError();
							break;
						default:
							hlsError = 'Stream error - please refresh';
							hls?.destroy();
							break;
					}
				}
			});

			hls.on(Hls.Events.FRAG_BUFFERED, () => {
				isBuffering = false;
				hlsError = null;
			});

		} else if (audioElement.canPlayType('application/vnd.apple.mpegurl')) {
			// Native HLS support (Safari)
			audioElement.src = streamUrl;
			audioElement.addEventListener('loadedmetadata', () => {
				if (hasStartedPlaying) {
					audioElement?.play();
				}
			});
		} else {
			hlsError = 'HLS is not supported in this browser';
		}
	}

	async function updateNowPlaying() {
		if (!station) return;

		try {
			const np = await api.getNowPlaying(station.id);
			nowPlaying = np;
			updateMediaSession();
		} catch (e) {
			console.error('Failed to update now playing:', e);
		}
	}

	function setupMediaSession() {
		if (!mediaSession) return;

		mediaSession.setActionHandler('play', () => {
			audioElement?.play();
		});

		mediaSession.setActionHandler('pause', () => {
			audioElement?.pause();
		});

		// Disable seeking for live stream
		mediaSession.setActionHandler('seekbackward', null);
		mediaSession.setActionHandler('seekforward', null);
		mediaSession.setActionHandler('seekto', null);
		mediaSession.setActionHandler('previoustrack', null);
		mediaSession.setActionHandler('nexttrack', null);

		if ('setPositionState' in mediaSession) {
			mediaSession.setPositionState();
		}
	}

	async function updateMediaSession() {
		if (!mediaSession || !nowPlaying) return;

		const coverUrl = nowPlaying.track.albumArt
			? (nowPlaying.track.albumArt.startsWith('http')
				? nowPlaying.track.albumArt
				: `${window.location.origin}${nowPlaying.track.albumArt}`)
			: `${window.location.origin}/api/v1/navidrome/cover/${nowPlaying.track.id}`;

		try {
			const response = await fetch(coverUrl);
			if (!response.ok) throw new Error('Failed to fetch cover');

			const blob = await response.blob();
			const blobUrl = URL.createObjectURL(blob);

			mediaSession.metadata = new MediaMetadata({
				title: nowPlaying.track.title,
				artist: nowPlaying.track.artist,
				album: nowPlaying.track.album || station?.name || 'Navidrome Radio',
				artwork: [
					{ src: blobUrl, sizes: '96x96', type: blob.type },
					{ src: blobUrl, sizes: '128x128', type: blob.type },
					{ src: blobUrl, sizes: '192x192', type: blob.type },
					{ src: blobUrl, sizes: '256x256', type: blob.type },
					{ src: blobUrl, sizes: '384x384', type: blob.type },
					{ src: blobUrl, sizes: '512x512', type: blob.type }
				]
			});
		} catch {
			mediaSession.metadata = new MediaMetadata({
				title: nowPlaying.track.title,
				artist: nowPlaying.track.artist,
				album: nowPlaying.track.album || station?.name || 'Navidrome Radio',
				artwork: [{ src: coverUrl, sizes: '512x512', type: 'image/jpeg' }]
			});
		}

		mediaSession.playbackState = audioElement?.paused ? 'paused' : 'playing';

		if ('setPositionState' in mediaSession) {
			mediaSession.setPositionState();
		}
	}

	function updateProgress() {
		if (!nowPlaying) return;

		const startedAt = new Date(nowPlaying.started_at).getTime();
		const now = Date.now();
		const elapsed = (now - startedAt) / 1000;

		currentPosition = Math.min(elapsed, nowPlaying.track.duration);
	}

	function formatTime(seconds: number): string {
		const mins = Math.floor(seconds / 60);
		const secs = Math.floor(seconds % 60);
		return `${mins}:${secs.toString().padStart(2, '0')}`;
	}

	let isSkipping = $state(false);

	async function handleSkip() {
		if (!station || !authStore.isAdmin || isSkipping) return;

		isSkipping = true;

		try {
			// Immediately stop audio to prevent hearing old track
			if (audioElement) {
				audioElement.pause();
				audioElement.currentTime = 0;
			}

			// Destroy HLS instance immediately
			if (hls) {
				hls.destroy();
				hls = null;
			}

			// Call skip API - backend waits for first segment to be ready
			await api.skipTrack(station.id);

			// Update UI with new track info
			await updateNowPlaying();

			// Re-initialize HLS and start playback
			initHls();
		} catch (e) {
			console.error('Failed to skip track:', e);
			alert('Failed to skip track: ' + (e instanceof Error ? e.message : 'Unknown error'));
		} finally {
			isSkipping = false;
		}
	}

	function startListening() {
		if (!audioElement || !station) return;

		hasStartedPlaying = true;

		// Add buffering event listeners only once to avoid accumulation
		if (!audioListenersAttached) {
			audioElement.addEventListener('waiting', () => {
				isBuffering = true;
			});
			audioElement.addEventListener('playing', () => {
				isBuffering = false;
			});
			audioElement.addEventListener('canplay', () => {
				isBuffering = false;
			});
			audioListenersAttached = true;
		}

		// Initialize HLS stream
		initHls();

		audioElement.muted = isMuted;
		audioElement.volume = 1.0;

		// Start playback
		audioElement.play().catch((e) => {
			console.error('Playback failed:', e);
			hlsError = 'Click to retry playback';
		});

		// Start listener heartbeat
		startHeartbeat();
	}

	function startHeartbeat() {
		if (!station || heartbeatInterval) return;

		sendHeartbeat();
		heartbeatInterval = setInterval(sendHeartbeat, 10000);
	}

	async function sendHeartbeat() {
		if (!station || !sessionId) return;

		try {
			const result = await api.listenerHeartbeat(station.id, sessionId);
			if (nowPlaying) {
				nowPlaying.listeners = result.listeners;
			}
		} catch (e) {
			console.error('Failed to send heartbeat:', e);
		}
	}

	function toggleMute() {
		if (!audioElement) return;

		if (!hasStartedPlaying) {
			startListening();
			return;
		}

		isMuted = !isMuted;
		audioElement.muted = isMuted;
		localStorage.setItem('radio_muted', isMuted.toString());

		if (!isMuted && audioElement.paused) {
			audioElement.play();
		}
	}

</script>

<svelte:head>
	<style>
		html, body {
			overflow: hidden !important;
			height: 100% !important;
			margin: 0 !important;
			padding: 0 !important;
		}
	</style>
</svelte:head>

{#if loading}
	<div class="fixed inset-0 flex items-center justify-center bg-gray-900">
		<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
	</div>
{:else if error}
	<div class="fixed inset-0 flex items-center justify-center bg-gray-900">
		<div class="text-center">
			<p class="text-red-500 text-xl mb-4">{error}</p>
			<a href="/" class="text-blue-400 hover:text-blue-300">Back to stations</a>
		</div>
	</div>
{:else if station && nowPlaying}
	<div class="fixed inset-0 flex flex-col bg-gray-900 text-white p-2 overflow-hidden">
		<!-- HLS Audio Element -->
		<audio
			bind:this={audioElement}
			crossorigin="anonymous"
		></audio>

		<!-- Station name at top -->
		<div class="text-center shrink-0 leading-none">
			<a href="/" class="text-blue-400 hover:text-blue-300 text-xs">← Back</a>
			<h1 class="text-base md:text-lg font-bold">{station.name}</h1>
			{#if hlsError}
				<p class="text-yellow-500 text-xs">{hlsError}</p>
			{/if}
			{#if isBuffering}
				<p class="text-gray-400 text-xs">Buffering...</p>
			{/if}
		</div>

		<!-- Now playing - centered -->
		<div class="flex-1 flex flex-col items-center justify-center gap-1 min-h-0">
			<!-- Start listening overlay -->
			{#if !hasStartedPlaying}
				<div class="fixed inset-0 bg-black bg-opacity-90 flex items-center justify-center z-50 backdrop-blur-sm">
					<div class="text-center space-y-4 p-6">
						<div class="text-white">
							<svg class="w-20 h-20 mx-auto mb-3 animate-pulse" fill="currentColor" viewBox="0 0 20 20">
								<path
									fill-rule="evenodd"
									d="M10 18a8 8 0 100-16 8 8 0 000 16zM9.555 7.168A1 1 0 008 8v4a1 1 0 001.555.832l3-2a1 1 0 000-1.664l-3-2z"
									clip-rule="evenodd"
								/>
							</svg>
							<p class="text-base md:text-lg text-gray-300 mb-1">{nowPlaying.track.title}</p>
							<p class="text-sm text-gray-400">{nowPlaying.track.artist}</p>
						</div>
						<button
							onclick={startListening}
							class="px-12 py-4 bg-gradient-to-r from-blue-600 to-purple-600 hover:from-blue-700 hover:to-purple-700 text-white text-xl font-bold rounded-full transition-all transform hover:scale-105 active:scale-95 shadow-2xl"
						>
							Play Radio
						</button>
						<p class="text-xs text-gray-500">Live HLS Stream</p>
					</div>
				</div>
			{/if}

			<!-- 3D Dancing Animals Visualizer -->
			<div class="relative w-full flex-1 min-h-[200px] max-h-[60vh]">
				<DancingAnimals
					{audioElement}
					isPlaying={hasStartedPlaying && !isMuted}
				/>
				<!-- Album art overlay in corner -->
				{#if nowPlaying.track.albumArt && hasStartedPlaying}
					<img
						src={nowPlaying.track.albumArt}
						alt={nowPlaying.track.album}
						class="absolute bottom-2 right-2 w-12 h-12 md:w-16 md:h-16 rounded shadow-lg object-cover opacity-80 border border-gray-700"
					/>
				{/if}
			</div>

			<!-- Track info -->
			<div class="text-center w-full max-w-md px-2 shrink-0">
				<h2 class="text-base md:text-lg font-bold truncate">{nowPlaying.track.title}</h2>
				<p class="text-xs md:text-sm text-gray-300 truncate">{nowPlaying.track.artist}</p>
				<p class="text-xs text-gray-500 truncate">{nowPlaying.track.album}</p>
			</div>

			<!-- Progress bar -->
			<div class="w-full max-w-md px-4 shrink-0">
				<div class="flex items-center justify-between text-xs text-gray-500">
					<span>{formatTime(currentPosition)}</span>
					<span class="text-red-500 text-[10px] font-medium">LIVE</span>
					<span>{formatTime(nowPlaying.track.duration)}</span>
				</div>
				<div class="w-full bg-gray-700 rounded-full h-1 overflow-hidden">
					<div
						class="bg-red-500 h-full transition-all duration-200 ease-linear"
						style="width: {(currentPosition / nowPlaying.track.duration) * 100}%"
					></div>
				</div>
			</div>

			<!-- Mute/Unmute button -->
			<button
				onclick={toggleMute}
				class="w-12 h-12 md:w-14 md:h-14 bg-blue-600 hover:bg-blue-700 rounded-full flex items-center justify-center transition-colors active:scale-95 shrink-0"
				title={hasStartedPlaying ? (isMuted ? 'Unmute' : 'Mute') : 'Start Listening'}
			>
				{#if isMuted}
					<svg class="w-6 h-6" fill="currentColor" viewBox="0 0 20 20">
						<path
							fill-rule="evenodd"
							d="M9.383 3.076A1 1 0 0110 4v12a1 1 0 01-1.707.707L4.586 13H2a1 1 0 01-1-1V8a1 1 0 011-1h2.586l3.707-3.707a1 1 0 011.09-.217zM12.293 7.293a1 1 0 011.414 0L15 8.586l1.293-1.293a1 1 0 111.414 1.414L16.414 10l1.293 1.293a1 1 0 01-1.414 1.414L15 11.414l-1.293 1.293a1 1 0 01-1.414-1.414L13.586 10l-1.293-1.293a1 1 0 010-1.414z"
							clip-rule="evenodd"
						/>
					</svg>
				{:else}
					<svg class="w-6 h-6" fill="currentColor" viewBox="0 0 20 20">
						<path
							fill-rule="evenodd"
							d="M9.383 3.076A1 1 0 0110 4v12a1 1 0 01-1.707.707L4.586 13H2a1 1 0 01-1-1V8a1 1 0 011-1h2.586l3.707-3.707a1 1 0 011.09-.217zM14.657 2.929a1 1 0 011.414 0A9.972 9.972 0 0119 10a9.972 9.972 0 01-2.929 7.071 1 1 0 01-1.414-1.414A7.971 7.971 0 0017 10c0-2.21-.894-4.208-2.343-5.657a1 1 0 010-1.414zm-2.829 2.828a1 1 0 011.415 0A5.983 5.983 0 0115 10a5.984 5.984 0 01-1.757 4.243 1 1 0 01-1.415-1.415A3.984 3.984 0 0013 10a3.983 3.983 0 00-1.172-2.828 1 1 0 010-1.415z"
							clip-rule="evenodd"
						/>
					</svg>
				{/if}
			</button>
		</div>

		<!-- Controls at bottom -->
		<div class="shrink-0 flex items-center justify-center gap-4 pb-safe">
			<div class="flex items-center gap-1.5 text-sm text-gray-400">
				<svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
					<path
						d="M9 6a3 3 0 11-6 0 3 3 0 016 0zM17 6a3 3 0 11-6 0 3 3 0 016 0zM12.93 17c.046-.327.07-.66.07-1a6.97 6.97 0 00-1.5-4.33A5 5 0 0119 16v1h-6.07zM6 11a5 5 0 015 5v1H1v-1a5 5 0 015-5z"
					/>
				</svg>
				<span>{nowPlaying.listeners} {nowPlaying.listeners === 1 ? 'listener' : 'listeners'}</span>
			</div>

			{#if authStore.isAdmin}
				<button
					onclick={handleSkip}
					disabled={isSkipping}
					class="px-4 py-1.5 bg-orange-600 hover:bg-orange-700 disabled:bg-orange-800 disabled:cursor-wait rounded font-medium text-xs transition-colors active:scale-95"
				>
					{isSkipping ? 'Skipping...' : 'Skip'}
				</button>
			{/if}
		</div>
	</div>
{/if}

<style>
	.truncate {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.pb-safe {
		padding-bottom: env(safe-area-inset-bottom, 0);
	}
</style>
