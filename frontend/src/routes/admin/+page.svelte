<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, type CurationProgress, type EmbeddingProgress, type HybridCurationProgress, type SeedTrack, type SelectSeedsResponse } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.svelte';
	import type { Station } from '$lib/types';
	import EmbeddingVisualization from '$lib/components/EmbeddingVisualization.svelte';

	let stations = $state<Station[]>([]);
	let loading = $state(true);
	let showCreateForm = $state(false);

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

	// AI Curation progress (now using HybridCurationProgress for the new hybrid endpoint)
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
	let embeddingEventSource: EventSource | null = null;
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

	// Visualization state
	let showVisualization = $state(false);

	let listenerCountInterval: number;

	onMount(() => {
		if (!authStore.isAdmin) {
			goto('/');
			return;
		}

		Promise.all([loadStations(), loadAiCapabilities(), loadLibraryStats(), loadListenerCounts(), loadEmbeddingStatus()]);

		// Poll for listener counts every 5 seconds
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

	function handleStartEmbeddingIndex() {
		// Close any existing connection
		if (embeddingEventSource) {
			embeddingEventSource.close();
			embeddingEventSource = null;
		}

		indexingEmbeddings = true;
		isPaused = false;
		embeddingError = null;
		embeddingProgress = null;

		// Get auth token for SSE connection (EventSource can't send custom headers, so use query param)
		const token = localStorage.getItem('auth_token');
		if (!token) {
			embeddingError = 'Not authenticated';
			indexingEmbeddings = false;
			return;
		}

		// No max_tracks limit - process all remaining tracks
		const url = new URL('/api/v1/embeddings/index-stream', window.location.origin);
		url.searchParams.set('token', token);

		// Create EventSource connection
		embeddingEventSource = new EventSource(url.toString());

		embeddingEventSource.onmessage = (event) => {
			try {
				const progress = JSON.parse(event.data);
				embeddingProgress = progress;

				// Handle terminal states
				if (progress.type === 'completed') {
					indexingEmbeddings = false;
					isPaused = false;
					embeddingEventSource?.close();
					embeddingEventSource = null;
					loadEmbeddingStatus();
				} else if (progress.type === 'error') {
					indexingEmbeddings = false;
					isPaused = false;
					embeddingError = progress.message;
					embeddingEventSource?.close();
					embeddingEventSource = null;
				}
			} catch (e) {
				console.error('Failed to parse SSE message:', e);
			}
		};

		embeddingEventSource.onerror = (error) => {
			console.error('SSE connection error:', error);
			indexingEmbeddings = false;
			isPaused = false;
			embeddingError = 'Connection to embedding stream failed';
			embeddingEventSource?.close();
			embeddingEventSource = null;
		};
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
			// The SSE stream will complete with a "stopped" message
		} catch (e) {
			console.error('Failed to stop embeddings:', e);
			embeddingError = e instanceof Error ? e.message : 'Failed to stop';
		}
	}

	function handleSyncLibrary() {
		// Close any existing connection
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}

		syncing = true;
		syncError = null;
		syncProgress = null;

		// Get auth token for SSE connection (EventSource can't send custom headers, so use query param)
		const token = localStorage.getItem('auth_token');
		if (!token) {
			syncError = 'Not authenticated';
			syncing = false;
			return;
		}

		const url = new URL('/api/v1/library/sync-stream', window.location.origin);
		url.searchParams.set('token', token);

		// Create EventSource connection
		eventSource = new EventSource(url.toString());

		eventSource.onmessage = (event) => {
			try {
				const progress = JSON.parse(event.data);
				syncProgress = progress;

				// Handle terminal states
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

	// Cleanup on component unmount
	$effect(() => {
		return () => {
			if (eventSource) {
				eventSource.close();
				eventSource = null;
			}
			if (embeddingEventSource) {
				embeddingEventSource.close();
				embeddingEventSource = null;
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
			// Phase 1: Select seeds for user review
			curationProgress = {
				step: 'selecting_seeds',
				message: 'AI is selecting seed tracks...'
			};

			const seedsResult = await api.selectSeeds(description, 5);
			selectedSeeds = seedsResult.seeds;
			curationPhase = 'reviewing_seeds';
			analyzingDescription = false;
			curationProgress = null;

			// Use the AI-determined genres from the backend
			if (seedsResult.genres && seedsResult.genres.length > 0) {
				genresInput = seedsResult.genres.join(', ');
			} else {
				// Fallback: leave empty or use a generic message
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
			selectedSeeds = [...selectedSeeds]; // Trigger reactivity
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

			// Clear progress after a moment
			setTimeout(() => {
				curationProgress = null;
				curationPhase = 'idle';
			}, 1500);
		} catch (error) {
			console.error('Gap filling failed:', error);
			createError = error instanceof Error ? error.message : 'Failed to fill gaps between seeds';
			curationProgress = null;
			curationPhase = 'reviewing_seeds'; // Go back to seed review
		}
	}

	async function handleCreateStation(e: Event) {
		e.preventDefault();
		creating = true;
		createError = null;

		try {
			const genres = genresInput.split(',').map((g) => g.trim()).filter(Boolean);

			// Get track IDs from AI curation result if available
			const trackIds = aiResult?.tracks?.map(t => t.id) || [];

			await api.createStation({
				path: path.toLowerCase().replace(/\s+/g, '-'),
				name,
				description,
				genres,
				track_ids: trackIds
			});

			// Reset form
			path = '';
			name = '';
			description = '';
			genresInput = '';
			showCreateForm = false;
			useAI = false;
			aiResult = null;

			// Reload stations
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
		if (!confirm('Are you sure you want to delete this station?')) {
			return;
		}

		try {
			await api.deleteStation(id);
			await loadStations();
		} catch (e) {
			console.error('Failed to delete station:', e);
		}
	}

	// Playlist creation state
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
			// Auto-dismiss success after 5 seconds
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

		// Load tracks if not already loaded
		if (!stationTracks.get(stationId)) {
			loadingStationTracks = stationId;
			try {
				const result = await api.getStationTracks(stationId, 200);
				stationTracks.set(stationId, result.tracks);
				stationTracks = stationTracks; // Trigger reactivity
			} catch (e) {
				console.error('Failed to load station tracks:', e);
			} finally {
				loadingStationTracks = null;
			}
		}
	}
</script>

<div class="container mx-auto px-4 py-8 max-w-6xl">
	<div class="mb-8">
		<div class="flex items-start justify-between">
			<div>
				<h1 class="text-3xl font-bold mb-4">Admin Dashboard</h1>
				<p class="text-gray-400">Manage your radio stations</p>
			</div>

			<!-- AI Capabilities Badge -->
			{#if aiAvailable}
				<div class="bg-gradient-to-r from-purple-600 to-blue-600 rounded-lg p-4 max-w-sm">
					<div class="flex items-center gap-2 mb-2">
						<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
							></path>
						</svg>
						<span class="font-bold">AI Features Active</span>
					</div>
					<ul class="text-sm space-y-1 opacity-90">
						{#each aiFeatures as feature}
							<li>• {feature}</li>
						{/each}
					</ul>
				</div>
			{:else}
				<div class="bg-gray-700 rounded-lg p-4 max-w-sm">
					<p class="text-sm text-gray-400">
						<span class="font-semibold">AI features disabled</span><br />
						Configure ANTHROPIC_API_KEY to enable AI-powered station creation
					</p>
				</div>
			{/if}
		</div>
	</div>

	<!-- Library Sync Section -->
	<div class="mb-8 bg-gray-800 rounded-lg p-6">
		<div class="flex items-start justify-between mb-4">
			<div>
				<h2 class="text-xl font-bold mb-2">Library Management</h2>
				<p class="text-gray-400 text-sm">Sync your Navidrome library to enable AI-powered curation</p>
			</div>
			<button
				onclick={handleSyncLibrary}
				disabled={syncing}
				class="bg-green-600 hover:bg-green-700 disabled:bg-gray-600 text-white px-4 py-2 rounded font-semibold"
			>
				{#if syncing}
					<div class="flex items-center gap-2">
						<svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
							<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
							<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
						</svg>
						Syncing...
					</div>
				{:else}
					Sync Library
				{/if}
			</button>
		</div>

		{#if libraryStats}
			<div class="grid grid-cols-3 gap-4">
				<div class="bg-gray-700 rounded p-4">
					<div class="text-2xl font-bold text-blue-400">{libraryStats.total_tracks.toLocaleString()}</div>
					<div class="text-sm text-gray-400">Total Tracks</div>
				</div>
				<div class="bg-gray-700 rounded p-4">
					{#if embeddingStatus}
						{@const liveCompleted = indexingEmbeddings && embeddingProgress?.success_count !== undefined
							? embeddingStatus.tracks_with_embeddings + embeddingProgress.success_count
							: embeddingStatus.tracks_with_embeddings}
						{@const livePercent = embeddingStatus.total_tracks > 0
							? (liveCompleted / embeddingStatus.total_tracks) * 100
							: 0}
						<div class="text-2xl font-bold text-purple-400 {indexingEmbeddings ? 'animate-pulse' : ''}">
							{livePercent.toFixed(1)}%
						</div>
						<div class="text-sm text-gray-400">Audio Embeddings</div>
						<div class="text-xs text-gray-500 mt-1">
							{liveCompleted.toLocaleString()} / {embeddingStatus.total_tracks.toLocaleString()} tracks
							{#if indexingEmbeddings && embeddingProgress?.success_count}
								<span class="text-green-400">(+{embeddingProgress.success_count} new)</span>
							{/if}
						</div>
						<!-- Progress bar -->
						<div class="mt-2 bg-gray-600 rounded-full h-1.5 overflow-hidden">
							<div
								class="bg-purple-500 h-full transition-all duration-300"
								style="width: {livePercent}%"
							></div>
						</div>
					{:else}
						<div class="text-2xl font-bold text-gray-500">--</div>
						<div class="text-sm text-gray-400">Audio Embeddings</div>
					{/if}
				</div>
				<div class="bg-gray-700 rounded p-4">
					<div class="text-sm text-gray-400">Last Sync</div>
					<div class="text-sm font-semibold">{libraryStats.computed_at ? new Date(libraryStats.computed_at).toLocaleString() : 'Never'}</div>
				</div>
			</div>

			<!-- Embedding Controls -->
			{#if embeddingStatus && embeddingStatus.coverage_percent < 100}
				<div class="mt-4 flex flex-wrap items-center gap-3">
					{#if !indexingEmbeddings}
						<button
							onclick={handleStartEmbeddingIndex}
							disabled={syncing}
							class="bg-purple-600 hover:bg-purple-700 disabled:bg-gray-600 text-white px-4 py-2 rounded font-semibold text-sm"
						>
							Generate Audio Embeddings
						</button>
					{:else}
						<!-- Pause/Resume button -->
						{#if isPaused}
							<button
								onclick={handleResumeEmbeddings}
								class="bg-green-600 hover:bg-green-700 text-white px-4 py-2 rounded font-semibold text-sm flex items-center gap-2"
							>
								<svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
									<path d="M8 5v14l11-7z"/>
								</svg>
								Resume
							</button>
						{:else}
							<button
								onclick={handlePauseEmbeddings}
								class="bg-yellow-600 hover:bg-yellow-700 text-white px-4 py-2 rounded font-semibold text-sm flex items-center gap-2"
							>
								<svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
									<path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>
								</svg>
								Pause
							</button>
						{/if}
						<!-- Stop button -->
						<button
							onclick={handleStopEmbeddings}
							class="bg-red-600 hover:bg-red-700 text-white px-4 py-2 rounded font-semibold text-sm flex items-center gap-2"
						>
							<svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
								<path d="M6 6h12v12H6z"/>
							</svg>
							Stop
						</button>
					{/if}
					<span class="text-xs text-gray-500">
						{#if indexingEmbeddings}
							{#if isPaused}
								Paused - click Resume to continue
							{:else}
								Processing all remaining tracks...
							{/if}
						{:else}
							Analyzes audio files to enable ML-powered track similarity
						{/if}
					</span>
				</div>
			{/if}

			{#if embeddingError}
				<div class="mt-4 bg-red-900/30 border border-red-600/50 rounded p-4">
					<p class="text-red-200 text-sm">{embeddingError}</p>
				</div>
			{/if}

			<!-- Embedding Progress Display -->
			{#if embeddingProgress}
				<div class="mt-4 bg-purple-900/30 border border-purple-600/50 rounded p-4">
					<div class="flex items-start gap-3">
						<!-- Progress icon -->
						<div class="flex-shrink-0 mt-0.5">
							{#if embeddingProgress.type === 'error'}
								<div class="w-10 h-10 rounded-full bg-red-500/20 flex items-center justify-center">
									<svg class="w-6 h-6 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
									</svg>
								</div>
							{:else if embeddingProgress.type === 'completed'}
								<div class="w-10 h-10 rounded-full bg-green-500/20 flex items-center justify-center">
									<svg class="w-6 h-6 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path>
									</svg>
								</div>
							{:else}
								<div class="w-10 h-10 rounded-full bg-purple-500/20 flex items-center justify-center relative">
									<svg class="w-6 h-6 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"></path>
									</svg>
									<div class="absolute inset-0 rounded-full border-2 border-purple-400/50 animate-ping"></div>
								</div>
							{/if}
						</div>

						<div class="flex-1 min-w-0">
							<!-- Current status message -->
							<div class="font-semibold text-lg text-white">
								{#if embeddingProgress.type === 'started'}
									Starting Audio Analysis
								{:else if embeddingProgress.type === 'processing' || embeddingProgress.type === 'track_complete'}
									Generating Audio Embeddings
								{:else if embeddingProgress.type === 'track_error'}
									Processing (with errors)
								{:else if embeddingProgress.type === 'completed'}
									Audio Analysis Complete!
								{:else if embeddingProgress.type === 'error'}
									Error
								{:else}
									Processing...
								{/if}
							</div>

							<!-- Status message -->
							{#if embeddingProgress.message}
								<div class="text-sm text-purple-300 mt-1">{embeddingProgress.message}</div>
							{/if}

							<!-- Tracks currently being processed in parallel -->
							{#if embeddingProgress.in_progress && embeddingProgress.in_progress.length > 0}
								<div class="mt-2 space-y-1">
									<div class="text-xs text-purple-400 font-medium">Processing {embeddingProgress.in_progress.length} tracks in parallel:</div>
									<div class="flex flex-wrap gap-1.5">
										{#each embeddingProgress.in_progress as track}
											{@const parts = track.split(' - ')}
											{@const artist = parts[0] || ''}
											{@const title = parts.slice(1).join(' - ') || track}
											<div class="text-xs text-purple-200 bg-purple-900/50 border border-purple-700/50 rounded px-2 py-1.5 flex items-start gap-1.5 max-w-[220px]">
												<span class="text-purple-400 animate-pulse mt-0.5">♪</span>
												<div class="flex flex-col min-w-0">
													<span class="truncate font-medium" title={title}>{title}</span>
													<span class="truncate text-[10px] italic text-purple-300/70" title={artist}>{artist}</span>
												</div>
											</div>
										{/each}
									</div>
								</div>
							{:else if embeddingProgress.current_track}
								<!-- Fallback for legacy single track -->
								<div class="mt-2 text-sm text-purple-200 flex items-center gap-2 bg-purple-900/30 rounded p-2">
									<span class="text-purple-400">♪</span>
									<span class="truncate">{embeddingProgress.current_track}</span>
								</div>
							{/if}

							<!-- Track completion info -->
							{#if embeddingProgress.type === 'track_complete' && embeddingProgress.track_name}
								<div class="mt-2 text-sm text-green-300 flex items-center gap-2">
									<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
									</svg>
									<span class="truncate">{embeddingProgress.track_name}</span>
									{#if embeddingProgress.processing_time_ms}
										<span class="text-gray-500 text-xs">({embeddingProgress.processing_time_ms}ms)</span>
									{/if}
								</div>
							{/if}

							<!-- Progress bar -->
							{#if (embeddingProgress.total ?? 0) > 0}
								{@const progressCurrent = embeddingProgress.completed ?? embeddingProgress.current ?? 0}
								{@const progressTotal = embeddingProgress.total ?? 0}
								<div class="mt-3">
									<div class="flex justify-between text-xs text-gray-400 mb-1">
										<span>Completed {progressCurrent} of {progressTotal}</span>
										<span>{Math.round((progressCurrent / progressTotal) * 100)}%</span>
									</div>
									<div class="bg-gray-700 rounded-full h-2 overflow-hidden">
										<div
											class="h-full transition-all duration-300 {embeddingProgress.type === 'error' ? 'bg-red-500' : embeddingProgress.type === 'completed' ? 'bg-green-500' : 'bg-purple-500'}"
											style="width: {Math.round((progressCurrent / progressTotal) * 100)}%"
										></div>
									</div>
								</div>
							{/if}

							<!-- Success/Error counts -->
							{#if embeddingProgress.success_count !== undefined || embeddingProgress.error_count !== undefined}
								<div class="mt-3 flex gap-4 text-sm">
									{#if embeddingProgress.success_count !== undefined}
										<div class="flex items-center gap-1 text-green-400">
											<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
											</svg>
											<span>{embeddingProgress.success_count} successful</span>
										</div>
									{/if}
									{#if embeddingProgress.error_count !== undefined && embeddingProgress.error_count > 0}
										<div class="flex items-center gap-1 text-red-400">
											<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
											</svg>
											<span>{embeddingProgress.error_count} errors</span>
										</div>
									{/if}
								</div>
							{/if}

							<!-- Completion time -->
							{#if embeddingProgress.type === 'completed' && embeddingProgress.total_time_secs}
								<div class="mt-2 text-sm text-gray-400">
									Completed in {embeddingProgress.total_time_secs.toFixed(1)} seconds
								</div>
							{/if}
						</div>
					</div>
				</div>
			{/if}
		{:else}
			<div class="bg-yellow-900/30 border border-yellow-600/50 rounded p-4">
				<p class="text-yellow-200 text-sm">
					No library data found. Click "Sync Library" to index your Navidrome tracks.
				</p>
			</div>
		{/if}

		{#if syncError}
			<div class="mt-4 bg-red-900/30 border border-red-600/50 rounded p-4">
				<p class="text-red-200 text-sm">{syncError}</p>
			</div>
		{/if}

		<!-- Real-time Progress Display -->
		{#if syncProgress}
			<div class="mt-4 bg-blue-900/30 border border-blue-600/50 rounded p-4">
				<div class="flex items-center gap-3 mb-3">
					<svg class="w-5 h-5 text-blue-400 animate-pulse" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path>
					</svg>
					<div class="flex-1">
						<div class="text-sm font-semibold text-blue-200">{syncProgress.message}</div>
						{#if syncProgress.type === 'processing' && syncProgress.current && syncProgress.total}
							<div class="text-xs text-blue-300 mt-1">
								Processing {syncProgress.current.toLocaleString()} of {syncProgress.total.toLocaleString()} tracks
								{#if syncProgress.new_tracks}
									<span class="text-green-400">({syncProgress.new_tracks} new)</span>
								{/if}
							</div>
							<div class="mt-2 bg-gray-700 rounded-full h-2 overflow-hidden">
								<div
									class="bg-blue-500 h-full transition-all duration-300"
									style="width: {Math.round((syncProgress.current / syncProgress.total) * 100)}%"
								></div>
							</div>
							<div class="text-xs text-right text-gray-400 mt-1">
								{Math.round((syncProgress.current / syncProgress.total) * 100)}%
							</div>
						{:else if syncProgress.type === 'fetching' && syncProgress.iteration}
							<div class="text-xs text-blue-300 mt-1">
								Iteration {syncProgress.iteration}
							</div>
						{/if}
					</div>
				</div>
			</div>
		{/if}
	</div>

	<!-- Embedding Visualization Section -->
	{#if embeddingStatus && embeddingStatus.tracks_with_embeddings > 0}
		<div class="mb-8">
			<button
				onclick={() => (showVisualization = !showVisualization)}
				class="flex items-center gap-2 text-purple-400 hover:text-purple-300 mb-4 transition-colors"
			>
				<svg class="w-5 h-5 transition-transform {showVisualization ? 'rotate-90' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"></path>
				</svg>
				<span class="font-medium">{showVisualization ? 'Hide' : 'Show'} Embedding Visualization</span>
				<span class="text-xs text-gray-500">({embeddingStatus.tracks_with_embeddings.toLocaleString()} tracks)</span>
			</button>

			{#if showVisualization}
				<EmbeddingVisualization />
			{/if}
		</div>
	{/if}

	<!-- Create Station Button -->
	<div class="mb-8">
		<button
			onclick={() => (showCreateForm = !showCreateForm)}
			class="bg-blue-600 hover:bg-blue-700 text-white px-6 py-3 rounded-lg font-semibold"
		>
			{showCreateForm ? 'Cancel' : '+ Create New Station'}
		</button>
	</div>

	<!-- Create Form -->
	{#if showCreateForm}
		<div class="bg-gray-800 rounded-lg p-6 mb-8">
			<h2 class="text-xl font-bold mb-4">Create New Station</h2>

			{#if createError}
				<div class="bg-red-500/10 border border-red-500 text-red-500 px-4 py-3 rounded mb-4">
					{createError}
				</div>
			{/if}

			<form onsubmit={handleCreateStation}>
				<div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
					<div>
						<label for="name" class="block text-sm font-medium text-gray-300 mb-2">
							Station Name
						</label>
						<input
							type="text"
							id="name"
							bind:value={name}
							required
							class="w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
							placeholder="My Awesome Station"
						/>
					</div>

					<div>
						<label for="path" class="block text-sm font-medium text-gray-300 mb-2">
							URL Path
						</label>
						<input
							type="text"
							id="path"
							bind:value={path}
							required
							pattern="[a-z0-9-]+"
							class="w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
							placeholder="my-awesome-station"
						/>
						<p class="text-xs text-gray-400 mt-1">lowercase, numbers, and hyphens only</p>
					</div>
				</div>

				<div class="mb-4">
					<label for="description" class="block text-sm font-medium text-gray-300 mb-2">
						Description
						{#if aiAvailable}
							<span class="text-xs text-purple-400">(AI will analyze this)</span>
						{/if}
					</label>
					<textarea
						id="description"
						bind:value={description}
						required
						rows="3"
						class="w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="Describe the vibe and music style of this station... e.g., 'Chill vibes for late night coding sessions with ambient electronic music'"
					></textarea>
				</div>

				{#if aiAvailable && description.trim()}
					<div class="mb-4">
						<button
							type="button"
							onclick={handleAnalyzeDescription}
							disabled={analyzingDescription}
							class="flex items-center gap-2 px-4 py-2 bg-purple-600 hover:bg-purple-700 disabled:bg-purple-800 text-white text-sm rounded-lg"
						>
							{#if analyzingDescription}
								<svg class="w-4 h-4 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
									<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
									<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
								</svg>
								Searching your library...
							{:else}
								<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										stroke-width="2"
										d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
									></path>
								</svg>
								Find Matching Tracks with AI
							{/if}
						</button>
					</div>

					<!-- AI Curation Progress Display (Hybrid Curation) -->
					{#if curationProgress}
						{@const steps = ['started', 'checking_embeddings', 'selecting_seeds', 'seeds_selected', 'generating_embeddings', 'filling_gaps', 'completed']}
						{@const stepLabels = ['Starting', 'Checking Audio', 'AI Selecting Seeds', 'Seeds Ready', 'Generating Embeddings', 'Finding Similar', 'Complete']}
						{@const currentIndex = steps.indexOf(curationProgress.step)}
						<div class="mb-4 bg-gradient-to-r from-purple-900/40 to-blue-900/40 border border-purple-500/50 rounded-lg p-4 transition-all duration-500">
							<div class="flex items-start gap-4">
								<!-- Step indicator icon -->
								<div class="flex-shrink-0 mt-0.5">
									{#if curationProgress.step === 'error'}
										<div class="w-10 h-10 rounded-full bg-red-500/20 flex items-center justify-center">
											<svg class="w-6 h-6 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
											</svg>
										</div>
									{:else if curationProgress.step === 'completed'}
										<div class="w-10 h-10 rounded-full bg-green-500/20 flex items-center justify-center">
											<svg class="w-6 h-6 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path>
											</svg>
										</div>
									{:else if curationProgress.step === 'checking_embeddings'}
										<div class="w-10 h-10 rounded-full bg-blue-500/20 flex items-center justify-center relative">
											<svg class="w-6 h-6 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"></path>
											</svg>
											<div class="absolute inset-0 rounded-full border-2 border-blue-400/50 animate-ping"></div>
										</div>
									{:else if curationProgress.step === 'generating_embeddings'}
										<div class="w-10 h-10 rounded-full bg-orange-500/20 flex items-center justify-center relative">
											<svg class="w-6 h-6 text-orange-400 animate-pulse" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z"></path>
											</svg>
											<div class="absolute inset-0 rounded-full border-2 border-orange-400/50 animate-ping"></div>
										</div>
									{:else if curationProgress.step === 'filling_gaps'}
										<div class="w-10 h-10 rounded-full bg-cyan-500/20 flex items-center justify-center relative">
											<svg class="w-6 h-6 text-cyan-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"></path>
											</svg>
											<div class="absolute inset-0 rounded-full border-2 border-cyan-400/50 animate-ping"></div>
										</div>
									{:else}
										<div class="w-10 h-10 rounded-full bg-purple-500/20 flex items-center justify-center relative">
											<svg class="w-6 h-6 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"></path>
											</svg>
											<div class="absolute inset-0 rounded-full border-2 border-purple-400/50 animate-ping"></div>
										</div>
									{/if}
								</div>

								<div class="flex-1 min-w-0">
									<!-- Current step message -->
									<div class="font-semibold text-lg text-white">{curationProgress.message}</div>

									<!-- Embedding coverage display -->
									{#if curationProgress.step === 'checking_embeddings' && curationProgress.coverage_percent !== undefined}
										<div class="mt-2 text-sm text-blue-300 flex items-center gap-2">
											<span>🎵</span>
											<span>Audio embedding coverage: {curationProgress.coverage_percent.toFixed(1)}%</span>
										</div>
									{/if}

									<!-- Seeds selected display -->
									{#if curationProgress.step === 'seeds_selected' && curationProgress.seeds}
										<div class="mt-2">
											<div class="text-sm text-purple-300 flex items-center gap-2 mb-1">
												<span>🌱</span>
												<span>Seed tracks selected: {curationProgress.count}</span>
											</div>
											<div class="flex flex-wrap gap-1 mt-1">
												{#each curationProgress.seeds.slice(0, 5) as seed}
													<span class="text-xs bg-purple-800/50 text-purple-200 px-2 py-1 rounded-full truncate max-w-48">
														{seed}
													</span>
												{/each}
												{#if curationProgress.seeds.length > 5}
													<span class="text-xs text-gray-400">+{curationProgress.seeds.length - 5} more</span>
												{/if}
											</div>
										</div>
									{/if}

									<!-- Generating embeddings display -->
									{#if curationProgress.step === 'generating_embeddings'}
										<div class="mt-2 text-sm text-orange-300">
											<div class="flex items-center gap-2 mb-2">
												<span>🧬</span>
												<span>Analyzing audio: {curationProgress.current} of {curationProgress.total}</span>
											</div>
											{#if curationProgress.track_name}
												<div class="text-xs text-orange-200 bg-orange-900/30 rounded px-2 py-1.5 flex items-center gap-2">
													<span class="text-orange-400 animate-pulse">♪</span>
													<span class="truncate">{curationProgress.track_name}</span>
												</div>
											{/if}
											<!-- Mini progress bar -->
											{#if curationProgress.total && curationProgress.total > 0}
												<div class="mt-2 bg-gray-700 rounded-full h-1.5 overflow-hidden">
													<div
														class="bg-orange-500 h-full transition-all duration-300"
														style="width: {((curationProgress.current || 0) / curationProgress.total) * 100}%"
													></div>
												</div>
											{/if}
										</div>
									{/if}

									<!-- Filling gaps display -->
									{#if curationProgress.step === 'filling_gaps'}
										<div class="mt-2 text-sm text-cyan-300">
											<div class="flex items-center gap-2 mb-1">
												<span>🔗</span>
												<span>Segment {curationProgress.segment} of {curationProgress.total_segments}</span>
											</div>
											{#if curationProgress.from_seed && curationProgress.to_seed}
												<div class="text-xs text-gray-400 bg-gray-800/50 rounded px-2 py-1">
													{curationProgress.from_seed} → {curationProgress.to_seed}
												</div>
											{/if}
										</div>
									{/if}

									<!-- Final track count -->
									{#if curationProgress.step === 'completed' && curationProgress.total_tracks}
										<div class="mt-2 text-sm text-green-300 font-medium flex items-center gap-2">
											<span>✅</span>
											<span>{curationProgress.total_tracks} tracks curated</span>
											{#if curationProgress.method}
												<span class="text-xs text-gray-400">({curationProgress.method} mode)</span>
											{/if}
										</div>
										{#if curationProgress.seed_count && curationProgress.filled_count}
											<div class="text-xs text-gray-400 mt-1">
												{curationProgress.seed_count} seeds + {curationProgress.filled_count} similar tracks
											</div>
										{/if}
									{/if}

									<!-- Step progress indicator -->
									<div class="mt-4">
										<div class="flex items-center gap-1 mb-2">
											{#each steps as step, i}
												<div
													class="h-2 flex-1 rounded-full transition-all duration-500 ease-out {i < currentIndex ? 'bg-purple-500' : i === currentIndex ? (curationProgress.step === 'completed' ? 'bg-green-500' : 'bg-purple-400') : 'bg-gray-700'}"
												></div>
											{/each}
										</div>
										<div class="flex justify-between text-xs">
											<span class="text-gray-500">Step {Math.max(1, currentIndex + 1)} of {steps.length}</span>
											<span class="text-purple-400 font-medium">{stepLabels[Math.max(0, currentIndex)] || 'Processing'}</span>
										</div>
									</div>
								</div>
							</div>
						</div>
					{/if}

					<!-- Seed Review UI -->
					{#if curationPhase === 'reviewing_seeds' && selectedSeeds.length > 0}
						<div class="mb-4 bg-gradient-to-r from-purple-900/40 to-indigo-900/40 border border-purple-500/50 rounded-lg p-4">
							<div class="flex items-center gap-2 mb-4">
								<div class="w-8 h-8 rounded-full bg-purple-500/20 flex items-center justify-center">
									<svg class="w-5 h-5 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"></path>
									</svg>
								</div>
								<div>
									<h4 class="font-semibold text-white">Review Seed Tracks</h4>
									<p class="text-xs text-gray-400">These anchor tracks will define your playlist's vibe. Click regenerate if any don't fit.</p>
								</div>
							</div>

							<div class="space-y-2 mb-4">
								{#each selectedSeeds as seed, i}
									<div class="flex items-center gap-3 bg-gray-800/50 rounded-lg p-3 border border-gray-700/50 hover:border-purple-500/30 transition-colors">
										<div class="w-8 h-8 rounded-full bg-purple-600/30 flex items-center justify-center flex-shrink-0">
											<span class="text-purple-300 font-bold text-sm">{i + 1}</span>
										</div>
										<div class="flex-1 min-w-0">
											<div class="font-medium text-white truncate">{seed.title}</div>
											<div class="text-sm text-gray-400 truncate">{seed.artist}</div>
										</div>
										<div class="text-xs text-gray-500 hidden sm:block truncate max-w-32">
											{seed.album}
										</div>
										<button
											type="button"
											onclick={() => handleRegenerateSeed(i)}
											disabled={regeneratingIndex !== null}
											class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg transition-all {regeneratingIndex === i ? 'bg-purple-600 text-white' : 'bg-gray-700 text-gray-300 hover:bg-gray-600 hover:text-white'} disabled:opacity-50"
										>
											{#if regeneratingIndex === i}
												<svg class="w-3.5 h-3.5 animate-spin" fill="none" viewBox="0 0 24 24">
													<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
													<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
												</svg>
												<span>Finding...</span>
											{:else}
												<svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
													<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path>
												</svg>
												<span>Regenerate</span>
											{/if}
										</button>
									</div>
								{/each}
							</div>

							<div class="flex items-center justify-between pt-3 border-t border-gray-700/50">
								<p class="text-xs text-gray-400">
									Happy with the seeds? Click continue to build the full playlist.
								</p>
								<button
									type="button"
									onclick={handleApproveSeedsAndFillGaps}
									class="flex items-center gap-2 px-4 py-2 bg-gradient-to-r from-purple-600 to-blue-600 hover:from-purple-500 hover:to-blue-500 text-white font-medium rounded-lg transition-all"
								>
									<span>Continue</span>
									<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7l5 5m0 0l-5 5m5-5H6"></path>
									</svg>
								</button>
							</div>
						</div>
					{/if}
				{/if}

				{#if aiResult}
					<div class="mb-4 bg-gradient-to-r from-purple-900/40 to-blue-900/40 border border-purple-500/50 rounded-lg p-4">
						<div class="flex items-center justify-between mb-2">
							<div class="flex items-center gap-2">
								<svg class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
									<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
								</svg>
								<span class="font-semibold text-green-400">
									Found {aiResult.tracks_found} matching tracks in your library!
								</span>
							</div>
							{#if aiResult.tracks.length > 5}
								<button
									type="button"
									onclick={() => showFullPlaylist = !showFullPlaylist}
									class="text-sm px-3 py-1 bg-purple-600 hover:bg-purple-700 text-white rounded"
								>
									{showFullPlaylist ? 'Show Less' : `View All ${aiResult.tracks.length} Tracks`}
								</button>
							{/if}
						</div>
						{#if aiResult.sample_tracks.length > 0}
							<div class="mt-2">
								<p class="text-sm text-gray-300 mb-1">
									{showFullPlaylist ? 'All tracks:' : 'Sample tracks:'}
								</p>
								<div class="max-h-96 overflow-y-auto">
									<ul class="text-sm text-gray-400 space-y-1">
										{#each (showFullPlaylist ? aiResult.tracks : aiResult.tracks.slice(0, 5)) as track}
											<li class="flex items-start gap-2">
												<span class="text-purple-400">♪</span>
												<span>{track.artist} - {track.title}</span>
											</li>
										{/each}
									</ul>
								</div>
							</div>
						{/if}
					</div>
				{/if}

				<div class="mb-6">
					<label for="genres" class="block text-sm font-medium text-gray-300 mb-2">
						Genres (comma separated)
						{#if analyzingDescription}
							<span class="text-xs text-purple-400 animate-pulse">AI is analyzing...</span>
						{/if}
					</label>
					<input
						type="text"
						id="genres"
						bind:value={genresInput}
						required
						class="w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="Rock, Alternative, Indie"
					/>
					<p class="text-xs text-gray-400 mt-1">
						{aiAvailable
							? 'AI can auto-fill this based on your description'
							: 'Manually enter genres separated by commas'}
					</p>
				</div>

				<button
					type="submit"
					disabled={creating}
					class="bg-green-600 hover:bg-green-700 disabled:bg-green-800 text-white px-6 py-2 rounded-lg font-semibold"
				>
					{creating ? 'Creating...' : 'Create Station'}
				</button>
			</form>
		</div>
	{/if}

	<!-- Stations List -->
	{#if loading}
		<div class="flex items-center justify-center py-12">
			<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
		</div>
	{:else if stations.length === 0}
		<div class="text-center text-gray-400 py-12">
			<p>No stations yet. Create your first one!</p>
		</div>
	{:else}
		<div class="space-y-4">
			{#each stations as station}
				<div class="bg-gray-800 rounded-lg p-6">
					<div class="flex items-start justify-between">
						<div class="flex-1">
							<div class="flex items-center gap-3 mb-2">
								<h3 class="text-xl font-bold">{station.name}</h3>
								{#if station.active}
									<span
										class="px-2 py-1 bg-green-500 text-white text-xs font-semibold rounded-full flex items-center gap-1"
									>
										<span class="w-2 h-2 bg-white rounded-full animate-pulse"></span>
										Live
									</span>
									<!-- Listener count -->
									<span class="px-2 py-1 bg-blue-600 text-white text-xs font-semibold rounded-full flex items-center gap-1">
										<svg class="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
											<path d="M9 6a3 3 0 11-6 0 3 3 0 016 0zM17 6a3 3 0 11-6 0 3 3 0 016 0zM12.93 17c.046-.327.07-.66.07-1a6.97 6.97 0 00-1.5-4.33A5 5 0 0119 16v1h-6.07zM6 11a5 5 0 015 5v1H1v-1a5 5 0 015-5z" />
										</svg>
										{listenerCounts[station.id] || 0} {(listenerCounts[station.id] || 0) === 1 ? 'listener' : 'listeners'}
									</span>
								{:else}
									<span class="px-2 py-1 bg-gray-600 text-gray-300 text-xs font-semibold rounded-full">
										Offline
									</span>
								{/if}
							</div>

							<p class="text-sm text-gray-400 mb-3">{station.description}</p>

							<div class="flex flex-wrap gap-2 mb-3">
								{#each station.genres as genre}
									<span class="px-2 py-1 bg-gray-700 text-gray-300 text-xs rounded">{genre}</span>
								{/each}
							</div>

							<p class="text-xs text-gray-500">
								Path: <code class="bg-gray-700 px-2 py-1 rounded">/station/{station.path}</code>
							</p>
						</div>

						<div class="flex flex-col gap-2 ml-4">
							{#if station.active}
								<button
									onclick={() => handleStopStation(station.id)}
									class="px-4 py-2 bg-orange-600 hover:bg-orange-700 text-white text-sm rounded-lg whitespace-nowrap"
								>
									Stop
								</button>
							{:else}
								<button
									onclick={() => handleStartStation(station.id)}
									class="px-4 py-2 bg-green-600 hover:bg-green-700 text-white text-sm rounded-lg whitespace-nowrap"
								>
									Start
								</button>
							{/if}

							<a
								href="/station/{station.path}"
								class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm rounded-lg text-center whitespace-nowrap"
							>
								Listen
							</a>

							<button
								onclick={() => toggleStationTracks(station.id)}
								class="px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white text-sm rounded-lg whitespace-nowrap"
							>
								{expandedStationId === station.id ? 'Hide Tracks' : 'View Tracks'}
							</button>

							<button
								onclick={() => handleCreatePlaylist(station.id, station.name)}
								disabled={creatingPlaylist === station.id}
								class="px-4 py-2 bg-teal-600 hover:bg-teal-700 disabled:bg-teal-800 disabled:cursor-wait text-white text-sm rounded-lg whitespace-nowrap flex items-center gap-1"
							>
								{#if creatingPlaylist === station.id}
									<svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
										<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
										<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
									</svg>
									Creating...
								{:else}
									Export Playlist
								{/if}
							</button>

							<button
								onclick={() => handleDeleteStation(station.id)}
								class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white text-sm rounded-lg whitespace-nowrap"
							>
								Delete
							</button>
						</div>
					</div>

					<!-- Playlist creation success message -->
					{#if playlistSuccess?.stationId === station.id}
						<div class="mt-2 p-3 bg-teal-900/50 border border-teal-700 rounded-lg text-teal-300 text-sm flex items-center gap-2">
							<svg class="w-5 h-5 text-teal-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
							</svg>
							Created playlist "{playlistSuccess.name}" with {playlistSuccess.trackCount} tracks in Navidrome
						</div>
					{/if}

					<!-- Expandable Tracks Section -->
					{#if expandedStationId === station.id}
						<div class="mt-4 pt-4 border-t border-gray-700">
							<h4 class="text-sm font-semibold text-gray-300 mb-3 flex items-center gap-2">
								<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
									<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"></path>
								</svg>
								Station Playlist
							</h4>

							{#if loadingStationTracks === station.id}
								<div class="flex items-center justify-center py-8">
									<svg class="animate-spin h-6 w-6 text-purple-400" fill="none" viewBox="0 0 24 24">
										<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
										<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
									</svg>
									<span class="ml-2 text-gray-400">Loading tracks...</span>
								</div>
							{:else if stationTracks.get(station.id)?.length}
								<div class="max-h-80 overflow-y-auto">
									<table class="w-full text-sm">
										<thead class="text-xs text-gray-500 uppercase border-b border-gray-700 sticky top-0 bg-gray-800">
											<tr>
												<th class="text-left py-2 px-2">#</th>
												<th class="text-left py-2 px-2">Title</th>
												<th class="text-left py-2 px-2">Artist</th>
												<th class="text-left py-2 px-2">Album</th>
											</tr>
										</thead>
										<tbody>
											{#each stationTracks.get(station.id) || [] as track, i}
												<tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
													<td class="py-2 px-2 text-gray-500">{i + 1}</td>
													<td class="py-2 px-2 text-white font-medium truncate max-w-[200px]" title={track.title}>{track.title}</td>
													<td class="py-2 px-2 text-gray-400 truncate max-w-[150px]" title={track.artist}>{track.artist}</td>
													<td class="py-2 px-2 text-gray-500 truncate max-w-[150px]" title={track.album}>{track.album}</td>
												</tr>
											{/each}
										</tbody>
									</table>
								</div>
							{:else}
								<div class="text-center py-8 text-gray-500">
									<svg class="w-12 h-12 mx-auto mb-2 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"></path>
									</svg>
									<p>No tracks played yet</p>
									<p class="text-xs mt-1">Start the station to begin playing tracks</p>
								</div>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>
