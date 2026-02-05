<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, type CurationProgress, type EmbeddingProgress, type HybridCurationProgress, type SeedTrack, type SelectSeedsResponse, type EmbeddingPoint } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.svelte';
	import type { Station } from '$lib/types';

	// Visualization state
	let showVisualization = $state(false);
	let vizLoading = $state(false);
	let vizError = $state<string | null>(null);
	let vizPoints = $state<EmbeddingPoint[]>([]);
	let plotContainer: HTMLDivElement | undefined = $state();
	let Plotly: typeof import('plotly.js-dist-min') | null = $state(null);

	const categoryColors: Record<string, string> = {
		'Hip-Hop': '#e6194b',
		'Alternative': '#3cb44b',
		'Rock': '#4363d8',
		'Pop': '#f58231',
		'R&B/Soul': '#911eb4',
		'Jazz': '#46f0f0',
		'Electronic': '#f032e6',
		'Country': '#bcf60c',
		'Metal': '#fabebe',
		'Other': '#808080',
	};

	function categorizeGenre(genre: string | null): string {
		if (!genre) return 'Other';
		const g = genre.toLowerCase();
		if (g.includes('rap') || g.includes('hip hop') || g.includes('hip-hop') || g.includes('screwed')) return 'Hip-Hop';
		if (g.includes('alternative') || g.includes('indie')) return 'Alternative';
		if (g.includes('rock') && !g.includes('alternative')) return 'Rock';
		if (g.includes('pop')) return 'Pop';
		if (g.includes('r&b') || g.includes('soul')) return 'R&B/Soul';
		if (g.includes('jazz')) return 'Jazz';
		if (g.includes('electro') || g.includes('dance') || g.includes('electronic')) return 'Electronic';
		if (g.includes('country')) return 'Country';
		if (g.includes('metal')) return 'Metal';
		return 'Other';
	}

	async function loadVisualization() {
		if (!Plotly) {
			Plotly = await import('plotly.js-dist-min');
		}

		vizLoading = true;
		vizError = null;

		try {
			const response = await api.getEmbeddingsForVisualization();
			vizPoints = response.points;

			if (vizPoints.length === 0) {
				vizError = 'No embeddings found. Generate audio embeddings first.';
				vizLoading = false;
				return;
			}

			await renderPlot();
		} catch (e) {
			vizError = e instanceof Error ? e.message : 'Failed to load embeddings';
		} finally {
			vizLoading = false;
		}
	}

	async function renderPlot() {
		if (!plotContainer || !Plotly) return;

		const categorizedPoints: Record<string, { x: number[], y: number[], text: string[] }> = {};

		for (const p of vizPoints) {
			const category = categorizeGenre(p.genre);
			if (!categorizedPoints[category]) {
				categorizedPoints[category] = { x: [], y: [], text: [] };
			}
			categorizedPoints[category].x.push(p.x);
			categorizedPoints[category].y.push(p.y);
			categorizedPoints[category].text.push(`${p.title}\n${p.artist}\n${p.genre || 'Unknown'}`);
		}

		const traces = Object.entries(categorizedPoints).map(([category, data]) => ({
			x: data.x,
			y: data.y,
			mode: 'markers' as const,
			type: 'scatter' as const,
			name: category,
			marker: {
				size: 5,
				color: categoryColors[category] || '#808080',
				opacity: 0.8,
			},
			text: data.text,
			hoverinfo: 'text' as const,
			hoverlabel: {
				bgcolor: '#0a0a0a',
				bordercolor: categoryColors[category] || '#333',
				font: { color: '#e0e0e0', size: 10, family: 'monospace' }
			}
		}));

		traces.sort((a, b) => b.x.length - a.x.length);

		const layout: Partial<Plotly.Layout> = {
			paper_bgcolor: '#0a0a0a',
			plot_bgcolor: '#111',
			xaxis: {
				title: { text: 'PCA-1', font: { color: '#444', size: 10 } },
				color: '#444',
				gridcolor: '#222',
				zerolinecolor: '#333',
				tickfont: { size: 8, color: '#444' }
			},
			yaxis: {
				title: { text: 'PCA-2', font: { color: '#444', size: 10 } },
				color: '#444',
				gridcolor: '#222',
				zerolinecolor: '#333',
				tickfont: { size: 8, color: '#444' }
			},
			margin: { l: 40, r: 10, t: 10, b: 40 },
			hovermode: 'closest',
			legend: {
				font: { color: '#888', size: 9, family: 'monospace' },
				bgcolor: 'rgba(10, 10, 10, 0.9)',
				bordercolor: '#333',
				borderwidth: 1,
				x: 1,
				xanchor: 'right',
				y: 1,
			},
			showlegend: true,
		};

		const config = {
			responsive: true,
			displayModeBar: false,
			displaylogo: false,
		};

		await Plotly.newPlot(plotContainer, traces, layout, config);
	}

	async function toggleVisualization() {
		showVisualization = !showVisualization;
		if (showVisualization && vizPoints.length === 0) {
			await loadVisualization();
		}
	}

	$effect(() => {
		if (plotContainer && Plotly && vizPoints.length > 0 && showVisualization) {
			renderPlot();
		}
	});

	// Tab state
	let activeTab = $state<'library' | 'stations' | 'create'>('stations');

	let stations = $state<Station[]>([]);
	let loading = $state(true);

	// Create form fields
	let path = $state('');
	let name = $state('');
	let description = $state('');
	let genresInput = $state('');
	let creating = $state(false);
	let createError = $state<string | null>(null);

	// AI capabilities
	let aiAvailable = $state(false);
	let aiFeatures = $state<string[]>([]);
	let useAI = $state(false);
	let analyzingDescription = $state(false);
	let aiResult = $state<{
		tracks_found: number;
		tracks: Array<{ id: string; title: string; artist: string }>;
		sample_tracks: string[];
	} | null>(null);
	let showFullPlaylist = $state(false);

	// AI Curation progress
	let curationProgress = $state<HybridCurationProgress | null>(null);
	let curationAbort = $state<(() => void) | null>(null);
	let curationComplete = $state(false);

	// Two-phase curation state
	let curationPhase = $state<'idle' | 'selecting_seeds' | 'reviewing_seeds' | 'filling_gaps' | 'complete'>('idle');
	let selectedSeeds = $state<SeedTrack[]>([]);
	let regeneratingIndex = $state<number | null>(null);

	// Station track viewing
	let expandedStationId = $state<string | null>(null);
	let stationTracks = $state<Map<string, Array<{ id: string; title: string; artist: string; album?: string; played_at?: string }>>>(new Map());
	let loadingStationTracks = $state<string | null>(null);

	// Listener counts
	let listenerCounts = $state<Record<string, number>>({});

	// Library sync state
	let libraryStats = $state<{
		total_tracks: number;
		total_ai_analyzed: number;
		computed_at: string | null;
	} | null>(null);
	let syncing = $state(false);
	let syncError = $state<string | null>(null);

	// Audio embedding state
	let embeddingStatus = $state<{
		total_tracks: number;
		tracks_with_embeddings: number;
		coverage_percent: number;
		indexing_in_progress: boolean;
		control_state?: string;
	} | null>(null);
	let indexingEmbeddings = $state(false);
	let isPaused = $state(false);
	let embeddingError = $state<string | null>(null);
	let embeddingProgress = $state<EmbeddingProgress | null>(null);
	let embeddingAbortController: AbortController | null = null;
	let syncProgress = $state<{
		type: string;
		message: string;
		current?: number;
		total?: number;
		new_tracks?: number;
		iteration?: number;
		total_tracks?: number;
	} | null>(null);
	let eventSource: EventSource | null = null;

	let listenerCountInterval: number;

	onMount(() => {
		if (!authStore.isAdmin) {
			goto('/');
			return;
		}

		Promise.all([loadStations(), loadAiCapabilities(), loadLibraryStats(), loadListenerCounts(), loadEmbeddingStatus()]);

		listenerCountInterval = setInterval(loadListenerCounts, 5000);

		return () => {
			if (listenerCountInterval) {
				clearInterval(listenerCountInterval);
			}
		};
	});

	async function loadListenerCounts() {
		try {
			const result = await api.getListenerCounts();
			listenerCounts = result.counts;
		} catch (e) {
			console.error('Failed to load listener counts:', e);
		}
	}

	async function loadLibraryStats() {
		try {
			libraryStats = await api.getLibraryStats();
		} catch (e) {
			console.error('Failed to load library stats:', e);
		}
	}

	async function loadEmbeddingStatus() {
		try {
			embeddingStatus = await api.getEmbeddingStatus();
		} catch (e) {
			console.error('Failed to load embedding status:', e);
		}
	}

	async function handleStartEmbeddingIndex() {
		if (embeddingAbortController) {
			embeddingAbortController.abort();
			embeddingAbortController = null;
		}

		indexingEmbeddings = true;
		isPaused = false;
		embeddingError = null;
		embeddingProgress = null;

		const token = localStorage.getItem('auth_token');
		if (!token) {
			embeddingError = 'Not authenticated';
			indexingEmbeddings = false;
			return;
		}

		embeddingAbortController = new AbortController();

		try {
			const response = await fetch('/api/v1/embeddings/index-stream', {
				headers: {
					'Authorization': `Bearer ${token}`
				},
				signal: embeddingAbortController.signal
			});

			if (!response.ok) {
				throw new Error(`HTTP ${response.status}`);
			}

			const reader = response.body?.getReader();
			if (!reader) {
				throw new Error('No response body');
			}

			const decoder = new TextDecoder();
			let buffer = '';

			while (true) {
				const { done, value } = await reader.read();
				if (done) break;

				buffer += decoder.decode(value, { stream: true });
				const lines = buffer.split('\n');
				buffer = lines.pop() || '';

				for (const line of lines) {
					if (line.startsWith('data: ')) {
						try {
							const progress = JSON.parse(line.slice(6));
							embeddingProgress = progress;

							if (progress.type === 'completed') {
								indexingEmbeddings = false;
								isPaused = false;
								embeddingAbortController = null;
								loadEmbeddingStatus();
								return;
							} else if (progress.type === 'error') {
								indexingEmbeddings = false;
								isPaused = false;
								embeddingError = progress.message;
								embeddingAbortController = null;
								return;
							}
						} catch (e) {
							console.error('Failed to parse SSE message:', e);
						}
					}
				}
			}
		} catch (error) {
			if (error instanceof Error && error.name === 'AbortError') {
				return;
			}
			console.error('SSE connection error:', error);
			indexingEmbeddings = false;
			isPaused = false;
			embeddingError = 'Connection to embedding stream failed';
			embeddingAbortController = null;
		}
	}

	async function handlePauseEmbeddings() {
		try {
			await api.pauseEmbeddings();
			isPaused = true;
		} catch (e) {
			console.error('Failed to pause embeddings:', e);
			embeddingError = e instanceof Error ? e.message : 'Failed to pause';
		}
	}

	async function handleResumeEmbeddings() {
		try {
			await api.resumeEmbeddings();
			isPaused = false;
		} catch (e) {
			console.error('Failed to resume embeddings:', e);
			embeddingError = e instanceof Error ? e.message : 'Failed to resume';
		}
	}

	async function handleStopEmbeddings() {
		try {
			await api.stopEmbeddings();
		} catch (e) {
			console.error('Failed to stop embeddings:', e);
			embeddingError = e instanceof Error ? e.message : 'Failed to stop';
		}
	}

	function handleSyncLibrary() {
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}

		syncing = true;
		syncError = null;
		syncProgress = null;

		const token = localStorage.getItem('auth_token');
		if (!token) {
			syncError = 'Not authenticated';
			syncing = false;
			return;
		}

		const url = new URL('/api/v1/library/sync-stream', window.location.origin);
		url.searchParams.set('token', token);

		eventSource = new EventSource(url.toString());

		eventSource.onmessage = (event) => {
			try {
				const progress = JSON.parse(event.data);
				syncProgress = progress;

				if (progress.type === 'completed') {
					syncing = false;
					eventSource?.close();
					eventSource = null;
					loadLibraryStats();
				} else if (progress.type === 'error') {
					syncing = false;
					syncError = progress.message;
					eventSource?.close();
					eventSource = null;
				}
			} catch (e) {
				console.error('Failed to parse SSE message:', e);
			}
		};

		eventSource.onerror = (error) => {
			console.error('SSE connection error:', error);
			syncing = false;
			syncError = 'Connection to sync stream failed';
			eventSource?.close();
			eventSource = null;
		};
	}

	$effect(() => {
		return () => {
			if (eventSource) {
				eventSource.close();
				eventSource = null;
			}
			if (embeddingAbortController) {
				embeddingAbortController.abort();
				embeddingAbortController = null;
			}
		};
	});

	async function loadAiCapabilities() {
		try {
			const capabilities = await api.getAiCapabilities();
			aiAvailable = capabilities.available;
			aiFeatures = capabilities.features;
		} catch (e) {
			console.error('Failed to load AI capabilities:', e);
		}
	}

	async function loadStations() {
		try {
			stations = await api.getStations();
		} catch (e) {
			console.error('Failed to load stations:', e);
		} finally {
			loading = false;
		}
	}

	async function handleAnalyzeDescription() {
		if (!description.trim() || !aiAvailable) return;

		analyzingDescription = true;
		aiResult = null;
		createError = null;
		showFullPlaylist = false;
		curationProgress = null;
		curationComplete = false;
		curationPhase = 'selecting_seeds';
		selectedSeeds = [];

		try {
			curationProgress = {
				step: 'selecting_seeds',
				message: 'AI is selecting seed tracks...'
			};

			const seedsResult = await api.selectSeeds(description, 5);
			selectedSeeds = seedsResult.seeds;
			curationPhase = 'reviewing_seeds';
			analyzingDescription = false;
			curationProgress = null;

			if (seedsResult.genres && seedsResult.genres.length > 0) {
				genresInput = seedsResult.genres.join(', ');
			} else {
				genresInput = '';
			}
		} catch (error) {
			console.error('Seed selection failed:', error);
			createError = error instanceof Error ? error.message : 'AI seed selection failed. Make sure you have synced your library first.';
			analyzingDescription = false;
			curationProgress = null;
			curationPhase = 'idle';
		}
	}

	async function handleRegenerateSeed(index: number) {
		if (regeneratingIndex !== null) return;

		regeneratingIndex = index;
		try {
			const excludeIds = selectedSeeds.map(s => s.id);
			const result = await api.regenerateSeed(description, index, excludeIds);
			selectedSeeds[index] = result.seed;
			selectedSeeds = [...selectedSeeds];
		} catch (error) {
			console.error('Failed to regenerate seed:', error);
			createError = error instanceof Error ? error.message : 'Failed to regenerate seed';
		} finally {
			regeneratingIndex = null;
		}
	}

	async function handleApproveSeedsAndFillGaps() {
		curationPhase = 'filling_gaps';
		curationProgress = {
			step: 'filling_gaps',
			message: 'Finding similar tracks to fill gaps between seeds...'
		};

		try {
			const seedIds = selectedSeeds.map(s => s.id);
			const result = await api.fillGaps(description, seedIds, 200);

			curationPhase = 'complete';
			curationProgress = {
				step: 'completed',
				message: `Created playlist with ${result.track_ids.length} tracks!`,
				total_tracks: result.track_ids.length,
				seed_count: result.seed_count,
				filled_count: result.filled_count
			};

			aiResult = {
				tracks_found: result.track_ids.length,
				tracks: result.tracks,
				sample_tracks: result.tracks.slice(0, 5).map(t => `${t.artist} - ${t.title}`)
			};

			setTimeout(() => {
				curationProgress = null;
				curationPhase = 'idle';
			}, 1500);
		} catch (error) {
			console.error('Gap filling failed:', error);
			createError = error instanceof Error ? error.message : 'Failed to fill gaps between seeds';
			curationProgress = null;
			curationPhase = 'reviewing_seeds';
		}
	}

	async function handleCreateStation(e: Event) {
		e.preventDefault();
		creating = true;
		createError = null;

		try {
			const genres = genresInput.split(',').map((g) => g.trim()).filter(Boolean);
			const trackIds = aiResult?.tracks?.map(t => t.id) || [];

			await api.createStation({
				path: path.toLowerCase().replace(/\s+/g, '-'),
				name,
				description,
				genres,
				track_ids: trackIds
			});

			path = '';
			name = '';
			description = '';
			genresInput = '';
			useAI = false;
			aiResult = null;
			activeTab = 'stations';

			await loadStations();
		} catch (e) {
			createError = e instanceof Error ? e.message : 'Failed to create station';
		} finally {
			creating = false;
		}
	}

	async function handleStartStation(id: string) {
		try {
			await api.startStation(id);
			await loadStations();
		} catch (e) {
			console.error('Failed to start station:', e);
		}
	}

	async function handleStopStation(id: string) {
		try {
			await api.stopStation(id);
			await loadStations();
		} catch (e) {
			console.error('Failed to stop station:', e);
		}
	}

	async function handleDeleteStation(id: string) {
		if (!confirm('Delete this station?')) return;

		try {
			await api.deleteStation(id);
			await loadStations();
		} catch (e) {
			console.error('Failed to delete station:', e);
		}
	}

	let creatingPlaylist = $state<string | null>(null);
	let playlistSuccess = $state<{ stationId: string; name: string; trackCount: number } | null>(null);

	async function handleCreatePlaylist(stationId: string, stationName: string) {
		creatingPlaylist = stationId;
		playlistSuccess = null;

		try {
			const result = await api.createNavidromePlaylist(stationId);
			playlistSuccess = {
				stationId,
				name: result.name,
				trackCount: result.track_count
			};
			setTimeout(() => {
				if (playlistSuccess?.stationId === stationId) {
					playlistSuccess = null;
				}
			}, 5000);
		} catch (e) {
			console.error('Failed to create playlist:', e);
			alert(e instanceof Error ? e.message : 'Failed to create playlist');
		} finally {
			creatingPlaylist = null;
		}
	}

	async function toggleStationTracks(stationId: string) {
		if (expandedStationId === stationId) {
			expandedStationId = null;
			return;
		}

		expandedStationId = stationId;

		if (!stationTracks.get(stationId)) {
			loadingStationTracks = stationId;
			try {
				const result = await api.getStationTracks(stationId, 200);
				stationTracks.set(stationId, result.tracks);
				stationTracks = stationTracks;
			} catch (e) {
				console.error('Failed to load station tracks:', e);
			} finally {
				loadingStationTracks = null;
			}
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === '1') {
			activeTab = 'stations';
		} else if (e.key === '2') {
			activeTab = 'library';
		} else if (e.key === '3') {
			activeTab = 'create';
		}
	}
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="admin-container">
	<!-- Header -->
	<header class="header">
		<div class="header-content">
			<span class="header-border">┌──</span>
			<h1 class="title">ADMIN DASHBOARD</h1>
			<span class="header-border">──┐</span>
		</div>
		<div class="header-sub">
			<span class="sub-border">│</span>
			<a href="/" class="back-link">← Back to Radio</a>
			<span class="user-info">● {authStore.user?.username}</span>
			<span class="sub-border">│</span>
		</div>
	</header>

	<!-- Tab Navigation -->
	<nav class="tabs">
		<span class="tab-border">├─</span>
		<button class="tab" class:active={activeTab === 'stations'} onclick={() => activeTab = 'stations'}>
			[1] STATIONS ({stations.length})
		</button>
		<button class="tab" class:active={activeTab === 'library'} onclick={() => activeTab = 'library'}>
			[2] LIBRARY
		</button>
		<button class="tab" class:active={activeTab === 'create'} onclick={() => activeTab = 'create'}>
			[3] CREATE
		</button>
		{#if aiAvailable}
			<span class="ai-badge">● AI</span>
		{/if}
		<span class="tab-border-end">─┤</span>
	</nav>

	<!-- Main Content -->
	<main class="main-content">
		{#if loading}
			<div class="loading">
				<pre class="blink">LOADING...</pre>
			</div>
		{:else if activeTab === 'stations'}
			<!-- Stations Tab -->
			<div class="panel">
				<div class="panel-header">
					<span>┌─ STATIONS ────────────────────────────────────────────────────────────────────┐</span>
				</div>
				<div class="panel-content stations-grid">
					{#if stations.length === 0}
						<div class="empty-state">No stations yet. Press [3] to create one.</div>
					{:else}
						{#each stations as station, i}
							<div class="station-row" class:expanded={expandedStationId === station.id}>
								<span class="station-index">{(i + 1).toString().padStart(2, '0')}</span>
								<span class="station-status" class:live={station.active}>
									{station.active ? '●' : '○'}
								</span>
								<span class="station-name">{station.name}</span>
								<span class="station-listeners">[{listenerCounts[station.id] || 0}]</span>
								<span class="station-genres">
									{station.genres.slice(0, 2).join(', ')}
								</span>
								<div class="station-actions">
									{#if station.active}
										<button class="action-btn stop" onclick={() => handleStopStation(station.id)}>[STOP]</button>
									{:else}
										<button class="action-btn start" onclick={() => handleStartStation(station.id)}>[START]</button>
									{/if}
									<button class="action-btn" onclick={() => toggleStationTracks(station.id)}>
										[{expandedStationId === station.id ? 'HIDE' : 'TRACKS'}]
									</button>
									<button class="action-btn export" onclick={() => handleCreatePlaylist(station.id, station.name)} disabled={creatingPlaylist === station.id}>
										[EXPORT]
									</button>
									<button class="action-btn delete" onclick={() => handleDeleteStation(station.id)}>[DEL]</button>
								</div>

								{#if playlistSuccess?.stationId === station.id}
									<div class="success-msg">✓ Exported {playlistSuccess.trackCount} tracks</div>
								{/if}

								{#if expandedStationId === station.id}
									<div class="tracks-panel">
										{#if loadingStationTracks === station.id}
											<span class="blink">Loading tracks...</span>
										{:else if stationTracks.get(station.id)?.length}
											<div class="tracks-header">
												<span>{stationTracks.get(station.id)?.length} tracks</span>
											</div>
											<div class="tracks-list-scroll">
												{#each (stationTracks.get(station.id) || []) as track, ti}
													<div class="track-row">
														<span class="track-num">{(ti + 1).toString().padStart(3, '0')}</span>
														<span class="track-artist">{track.artist}</span>
														<span class="track-sep">-</span>
														<span class="track-title">{track.title}</span>
													</div>
												{/each}
											</div>
										{:else}
											<span class="empty">No tracks yet</span>
										{/if}
									</div>
								{/if}
							</div>
						{/each}
					{/if}
				</div>
				<div class="panel-footer">
					<span>└─────────────────────────────────────────────────────────────────────────────────┘</span>
				</div>
			</div>

		{:else if activeTab === 'library'}
			<!-- Library Tab -->
			<div class="panel">
				<div class="panel-header">
					<span>┌─ LIBRARY MANAGEMENT ─────────────────────────────────────────────────────────┐</span>
				</div>
				<div class="panel-content library-content">
					<div class="stats-row">
						<div class="stat-box">
							<span class="stat-label">TRACKS</span>
							<span class="stat-value">{libraryStats?.total_tracks.toLocaleString() || '--'}</span>
						</div>
						<div class="stat-box">
							<span class="stat-label">EMBEDDINGS</span>
							<span class="stat-value" class:processing={indexingEmbeddings}>
								{embeddingStatus?.coverage_percent.toFixed(1) || '--'}%
							</span>
							<span class="stat-sub">
								{embeddingStatus?.tracks_with_embeddings.toLocaleString() || 0} / {embeddingStatus?.total_tracks.toLocaleString() || 0}
							</span>
						</div>
						<div class="stat-box">
							<span class="stat-label">LAST SYNC</span>
							<span class="stat-value-sm">{libraryStats?.computed_at ? new Date(libraryStats.computed_at).toLocaleDateString() : 'Never'}</span>
						</div>
					</div>

					<div class="actions-row">
						<button class="tui-btn" onclick={handleSyncLibrary} disabled={syncing}>
							{syncing ? '[SYNCING...]' : '[SYNC LIBRARY]'}
						</button>
						{#if embeddingStatus && embeddingStatus.coverage_percent < 100}
							{#if !indexingEmbeddings}
								<button class="tui-btn" onclick={handleStartEmbeddingIndex} disabled={syncing}>
									[GENERATE EMBEDDINGS]
								</button>
							{:else}
								{#if isPaused}
									<button class="tui-btn" onclick={handleResumeEmbeddings}>[RESUME]</button>
								{:else}
									<button class="tui-btn" onclick={handlePauseEmbeddings}>[PAUSE]</button>
								{/if}
								<button class="tui-btn stop" onclick={handleStopEmbeddings}>[STOP]</button>
							{/if}
						{/if}
					</div>

					{#if syncProgress}
						<div class="progress-box">
							<span class="progress-label">{syncProgress.message}</span>
							{#if syncProgress.current && syncProgress.total}
								<div class="progress-bar">
									<div class="progress-fill" style="width: {(syncProgress.current / syncProgress.total) * 100}%"></div>
								</div>
								<span class="progress-pct">{Math.round((syncProgress.current / syncProgress.total) * 100)}%</span>
							{/if}
						</div>
					{/if}

					{#if embeddingProgress}
						<div class="progress-box embedding">
							<span class="progress-label">{embeddingProgress.message || 'Processing...'}</span>
							{#if embeddingProgress.in_progress && embeddingProgress.in_progress.length > 0}
								<div class="current-tracks">
									{#each embeddingProgress.in_progress.slice(0, 3) as track}
										<span class="current-track">♪ {track}</span>
									{/each}
								</div>
							{/if}
							{#if embeddingProgress.total && embeddingProgress.total > 0}
								{@const pct = ((embeddingProgress.completed || embeddingProgress.current || 0) / embeddingProgress.total) * 100}
								<div class="progress-bar">
									<div class="progress-fill" style="width: {pct}%"></div>
								</div>
								<span class="progress-pct">{Math.round(pct)}%</span>
							{/if}
							{#if embeddingProgress.success_count !== undefined}
								<span class="progress-stats">✓ {embeddingProgress.success_count} {embeddingProgress.error_count ? `✗ ${embeddingProgress.error_count}` : ''}</span>
							{/if}
						</div>
					{/if}

					{#if syncError}
						<div class="error-box">{syncError}</div>
					{/if}
					{#if embeddingError}
						<div class="error-box">{embeddingError}</div>
					{/if}

					<!-- Visualization Section -->
					{#if embeddingStatus && embeddingStatus.tracks_with_embeddings > 0}
						<div class="viz-section">
							<button class="viz-toggle" onclick={toggleVisualization}>
								<span class="viz-arrow">{showVisualization ? '▼' : '►'}</span>
								<span>EMBEDDING VISUALIZATION</span>
								<span class="viz-count">({embeddingStatus.tracks_with_embeddings.toLocaleString()} tracks)</span>
								{#if vizLoading}
									<span class="blink">loading...</span>
								{/if}
							</button>

							{#if showVisualization}
								<div class="viz-container">
									{#if vizError}
										<div class="error-box">{vizError}</div>
									{:else if vizLoading}
										<div class="viz-loading">
											<span class="blink">Loading visualization data...</span>
										</div>
									{:else}
										<div bind:this={plotContainer} class="viz-plot"></div>
										<div class="viz-help">Hover for track info. Click legend to filter genres.</div>
									{/if}
								</div>
							{/if}
						</div>
					{/if}
				</div>
				<div class="panel-footer">
					<span>└─────────────────────────────────────────────────────────────────────────────────┘</span>
				</div>
			</div>

		{:else if activeTab === 'create'}
			<!-- Create Tab -->
			<div class="panel">
				<div class="panel-header">
					<span>┌─ CREATE STATION ──────────────────────────────────────────────────────────────┐</span>
				</div>
				<div class="panel-content create-form">
					{#if createError}
						<div class="error-box">{createError}</div>
					{/if}

					<form onsubmit={handleCreateStation}>
						<div class="form-row">
							<label class="form-label">NAME:</label>
							<input type="text" bind:value={name} required class="form-input" placeholder="My Station" />
						</div>
						<div class="form-row">
							<label class="form-label">PATH:</label>
							<input type="text" bind:value={path} required pattern="[a-z0-9-]+" class="form-input" placeholder="my-station" />
						</div>
						<div class="form-row">
							<label class="form-label">DESC:</label>
							<textarea bind:value={description} required rows="2" class="form-input form-textarea" placeholder="Describe the vibe..."></textarea>
						</div>

						{#if aiAvailable && description.trim()}
							<div class="form-row">
								<button type="button" onclick={handleAnalyzeDescription} disabled={analyzingDescription} class="tui-btn ai">
									{analyzingDescription ? '[ANALYZING...]' : '[AI: FIND TRACKS]'}
								</button>
							</div>
						{/if}

						{#if curationPhase === 'reviewing_seeds' && selectedSeeds.length > 0}
							<div class="seeds-panel">
								<div class="seeds-header">SEED TRACKS (click to regenerate)</div>
								{#each selectedSeeds as seed, i}
									<div class="seed-row">
										<span class="seed-num">{i + 1}</span>
										<span class="seed-info">{seed.artist} - {seed.title}</span>
										<button type="button" class="seed-regen" onclick={() => handleRegenerateSeed(i)} disabled={regeneratingIndex !== null}>
											{regeneratingIndex === i ? '...' : '↻'}
										</button>
									</div>
								{/each}
								<button type="button" class="tui-btn" onclick={handleApproveSeedsAndFillGaps}>
									[APPROVE & BUILD PLAYLIST]
								</button>
							</div>
						{/if}

						{#if curationProgress}
							<div class="progress-box">
								<span class="progress-label">{curationProgress.message}</span>
							</div>
						{/if}

						{#if aiResult}
							<div class="result-box">
								<span class="result-header">✓ Found {aiResult.tracks_found} tracks</span>
								<div class="result-tracks-scroll">
									{#each aiResult.tracks as track, ti}
										<div class="result-track-row">
											<span class="track-num">{(ti + 1).toString().padStart(3, '0')}</span>
											<span class="track-artist">{track.artist}</span>
											<span class="track-sep">-</span>
											<span class="track-title">{track.title}</span>
										</div>
									{/each}
								</div>
							</div>
						{/if}

						<div class="form-row">
							<label class="form-label">GENRES:</label>
							<input type="text" bind:value={genresInput} required class="form-input" placeholder="Rock, Electronic, Jazz" />
						</div>

						<div class="form-row">
							<button type="submit" disabled={creating} class="tui-btn submit">
								{creating ? '[CREATING...]' : '[CREATE STATION]'}
							</button>
						</div>
					</form>
				</div>
				<div class="panel-footer">
					<span>└─────────────────────────────────────────────────────────────────────────────────┘</span>
				</div>
			</div>
		{/if}
	</main>

	<!-- Footer -->
	<footer class="footer">
		<span class="footer-border">├─</span>
		<span class="help">1:stations  2:library  3:create</span>
		<span class="footer-border-end">─┤</span>
	</footer>
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

	.admin-container {
		height: 100vh;
		display: flex;
		flex-direction: column;
		padding: 0.5rem 1rem;
		box-sizing: border-box;
		overflow: hidden;
	}

	/* Header */
	.header {
		flex-shrink: 0;
		text-align: center;
		border-bottom: 1px solid #333;
		padding-bottom: 0.5rem;
	}

	.header-content {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
	}

	.header-border {
		color: #333;
	}

	.title {
		font-size: 1.25rem;
		font-weight: bold;
		color: #00ff88;
		letter-spacing: 0.15em;
		margin: 0;
		text-shadow: 0 0 20px rgba(0, 255, 136, 0.3);
	}

	.header-sub {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		margin-top: 0.25rem;
		font-size: 0.75rem;
	}

	.sub-border {
		color: #333;
	}

	.back-link {
		color: #666;
		text-decoration: none;
		transition: color 0.15s;
	}

	.back-link:hover {
		color: #00ff88;
	}

	.user-info {
		color: #00ff88;
	}

	/* Tabs */
	.tabs {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0;
		flex-shrink: 0;
		border-bottom: 1px solid #333;
	}

	.tab-border, .tab-border-end {
		color: #333;
	}

	.tab-border-end {
		flex: 1;
		text-align: right;
	}

	.tab {
		background: transparent;
		border: none;
		color: #555;
		font-family: inherit;
		font-size: 0.8rem;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		transition: all 0.15s;
	}

	.tab:hover {
		color: #888;
	}

	.tab.active {
		color: #00ff88;
		text-shadow: 0 0 10px rgba(0, 255, 136, 0.3);
	}

	.ai-badge {
		color: #a855f7;
		font-size: 0.7rem;
	}

	/* Main Content */
	.main-content {
		flex: 1;
		overflow: hidden;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.loading {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.blink {
		animation: blink 1s infinite;
	}

	@keyframes blink {
		50% { opacity: 0.5; }
	}

	/* Panel */
	.panel {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.panel-header, .panel-footer {
		font-size: 0.7rem;
		color: #444;
		flex-shrink: 0;
		white-space: nowrap;
		overflow: hidden;
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		border-left: 1px solid #333;
		border-right: 1px solid #333;
		padding: 0.5rem;
		min-height: 0;
	}

	.panel-content::-webkit-scrollbar {
		width: 4px;
	}

	.panel-content::-webkit-scrollbar-track {
		background: #1a1a1a;
	}

	.panel-content::-webkit-scrollbar-thumb {
		background: #333;
	}

	/* Stations Grid */
	.stations-grid {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.station-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.4rem 0.5rem;
		background: #111;
		font-size: 0.8rem;
		flex-wrap: wrap;
	}

	.station-row.expanded {
		background: #1a1a1a;
	}

	.station-index {
		color: #444;
		width: 1.5rem;
	}

	.station-status {
		color: #666;
	}

	.station-status.live {
		color: #00ff88;
	}

	.station-name {
		color: #fff;
		flex: 1;
		min-width: 120px;
	}

	.station-listeners {
		color: #666;
		font-size: 0.7rem;
	}

	.station-genres {
		color: #555;
		font-size: 0.7rem;
		max-width: 150px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.station-actions {
		display: flex;
		gap: 0.25rem;
	}

	.action-btn {
		background: transparent;
		border: none;
		color: #666;
		font-family: inherit;
		font-size: 0.7rem;
		cursor: pointer;
		padding: 0.15rem 0.25rem;
		transition: color 0.15s;
	}

	.action-btn:hover {
		color: #888;
	}

	.action-btn.start:hover {
		color: #00ff88;
	}

	.action-btn.stop:hover {
		color: #ff8800;
	}

	.action-btn.export:hover {
		color: #00d4ff;
	}

	.action-btn.delete:hover {
		color: #ff4444;
	}

	.action-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.success-msg {
		width: 100%;
		color: #00ff88;
		font-size: 0.7rem;
		padding: 0.25rem 0.5rem;
	}

	.tracks-panel {
		width: 100%;
		padding: 0.5rem;
		background: #0a0a0a;
		border-top: 1px dashed #333;
		margin-top: 0.25rem;
	}

	.tracks-header {
		font-size: 0.65rem;
		color: #666;
		padding-bottom: 0.25rem;
		border-bottom: 1px dashed #222;
		margin-bottom: 0.25rem;
	}

	.tracks-list-scroll {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		max-height: 200px;
		overflow-y: auto;
		padding-right: 0.25rem;
	}

	.tracks-list-scroll::-webkit-scrollbar {
		width: 4px;
	}

	.tracks-list-scroll::-webkit-scrollbar-track {
		background: #111;
	}

	.tracks-list-scroll::-webkit-scrollbar-thumb {
		background: #333;
	}

	.track-row {
		display: flex;
		gap: 0.5rem;
		font-size: 0.7rem;
		color: #888;
	}

	.track-num {
		color: #444;
		width: 2rem;
		flex-shrink: 0;
	}

	.track-artist {
		color: #00ff88;
		max-width: 150px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.track-sep {
		color: #333;
		flex-shrink: 0;
	}

	.track-title {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.empty-state, .empty {
		color: #444;
		text-align: center;
		padding: 2rem;
	}

	/* Library Content */
	.library-content {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.stats-row {
		display: flex;
		gap: 1rem;
	}

	.stat-box {
		flex: 1;
		background: #111;
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		border: 1px solid #222;
	}

	.stat-label {
		font-size: 0.65rem;
		color: #666;
		margin-bottom: 0.25rem;
	}

	.stat-value {
		font-size: 1.5rem;
		color: #00ff88;
		font-weight: bold;
	}

	.stat-value.processing {
		animation: pulse 1s infinite;
	}

	@keyframes pulse {
		50% { opacity: 0.7; }
	}

	.stat-value-sm {
		font-size: 0.8rem;
		color: #888;
	}

	.stat-sub {
		font-size: 0.6rem;
		color: #555;
	}

	.actions-row {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.tui-btn {
		background: #1a1a1a;
		border: 1px solid #333;
		color: #888;
		font-family: inherit;
		font-size: 0.75rem;
		cursor: pointer;
		padding: 0.4rem 0.75rem;
		transition: all 0.15s;
	}

	.tui-btn:hover:not(:disabled) {
		background: #222;
		color: #00ff88;
		border-color: #00ff88;
	}

	.tui-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.tui-btn.ai {
		color: #a855f7;
		border-color: #a855f7;
	}

	.tui-btn.ai:hover:not(:disabled) {
		background: #1a0a2a;
		color: #c084fc;
	}

	.tui-btn.stop {
		color: #ff4444;
		border-color: #ff4444;
	}

	.tui-btn.stop:hover:not(:disabled) {
		background: #2a0a0a;
	}

	.tui-btn.submit {
		color: #00ff88;
		border-color: #00ff88;
	}

	.progress-box {
		background: #0a1a1a;
		border: 1px solid #1a3a3a;
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.progress-box.embedding {
		border-color: #2a1a3a;
		background: #0a0a1a;
	}

	.progress-label {
		font-size: 0.75rem;
		color: #00d4ff;
	}

	.progress-bar {
		height: 4px;
		background: #222;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: #00ff88;
		transition: width 0.3s;
	}

	.progress-pct {
		font-size: 0.65rem;
		color: #666;
		text-align: right;
	}

	.progress-stats {
		font-size: 0.65rem;
		color: #00ff88;
	}

	.current-tracks {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.current-track {
		font-size: 0.65rem;
		color: #a855f7;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.error-box {
		background: #2a0a0a;
		border: 1px solid #ff4444;
		color: #ff8888;
		padding: 0.5rem;
		font-size: 0.75rem;
	}

	/* Create Form */
	.create-form {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.form-row {
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
	}

	.form-label {
		width: 60px;
		font-size: 0.75rem;
		color: #666;
		text-align: right;
		padding-top: 0.4rem;
		flex-shrink: 0;
	}

	.form-input {
		flex: 1;
		background: #111;
		border: 1px solid #333;
		color: #fff;
		font-family: inherit;
		font-size: 0.8rem;
		padding: 0.4rem 0.5rem;
		outline: none;
		transition: border-color 0.15s;
	}

	.form-input:focus {
		border-color: #00ff88;
	}

	.form-input::placeholder {
		color: #444;
	}

	.form-textarea {
		resize: none;
	}

	.seeds-panel {
		background: #0a0a1a;
		border: 1px solid #2a1a3a;
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.seeds-header {
		font-size: 0.7rem;
		color: #a855f7;
	}

	.seed-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.75rem;
		padding: 0.25rem 0;
	}

	.seed-num {
		color: #a855f7;
		width: 1rem;
	}

	.seed-info {
		flex: 1;
		color: #ccc;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.seed-regen {
		background: transparent;
		border: none;
		color: #666;
		cursor: pointer;
		font-size: 0.8rem;
		padding: 0.1rem 0.25rem;
	}

	.seed-regen:hover:not(:disabled) {
		color: #a855f7;
	}

	.result-box {
		background: #0a1a0a;
		border: 1px solid #1a3a1a;
		padding: 0.75rem;
	}

	.result-header {
		font-size: 0.8rem;
		color: #00ff88;
		display: block;
		margin-bottom: 0.5rem;
		padding-bottom: 0.25rem;
		border-bottom: 1px dashed #1a3a1a;
	}

	.result-tracks-scroll {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		max-height: 180px;
		overflow-y: auto;
		padding-right: 0.25rem;
	}

	.result-tracks-scroll::-webkit-scrollbar {
		width: 4px;
	}

	.result-tracks-scroll::-webkit-scrollbar-track {
		background: #0a1a0a;
	}

	.result-tracks-scroll::-webkit-scrollbar-thumb {
		background: #1a3a1a;
	}

	.result-track-row {
		display: flex;
		gap: 0.5rem;
		font-size: 0.7rem;
		color: #888;
	}

	.result-track-row .track-num {
		color: #1a5a1a;
	}

	.result-track-row .track-artist {
		color: #00ff88;
	}

	/* Footer */
	.footer {
		display: flex;
		align-items: center;
		padding: 0.5rem 0;
		border-top: 1px solid #333;
		flex-shrink: 0;
	}

	.footer-border {
		color: #333;
	}

	.footer-border-end {
		flex: 1;
		text-align: right;
		color: #333;
	}

	.help {
		color: #444;
		font-size: 0.7rem;
	}

	/* Visualization Section */
	.viz-section {
		margin-top: 0.5rem;
		border: 1px solid #222;
		background: #0a0a0a;
	}

	.viz-toggle {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		background: #111;
		border: none;
		color: #888;
		font-family: inherit;
		font-size: 0.75rem;
		cursor: pointer;
		text-align: left;
		transition: all 0.15s;
	}

	.viz-toggle:hover {
		background: #1a1a1a;
		color: #00ff88;
	}

	.viz-arrow {
		color: #00ff88;
		width: 1rem;
	}

	.viz-count {
		color: #555;
		font-size: 0.65rem;
	}

	.viz-container {
		border-top: 1px solid #222;
		padding: 0.5rem;
	}

	.viz-plot {
		width: 100%;
		height: 300px;
		background: #0a0a0a;
	}

	.viz-loading {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 200px;
		color: #666;
	}

	.viz-help {
		text-align: center;
		font-size: 0.6rem;
		color: #444;
		padding-top: 0.5rem;
	}
</style>
