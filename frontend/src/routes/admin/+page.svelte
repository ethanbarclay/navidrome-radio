<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, type CurationProgress } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.svelte';
	import type { Station } from '$lib/types';

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

	// AI Curation progress
	let curationProgress = $state<CurationProgress | null>(null);
	let curationAbort = $state<(() => void) | null>(null);
	let curationComplete = $state(false);

	// Station track viewing
	let expandedStationId = $state<string | null>(null);
	let stationTracks = $state<Map<string, Array<{ id: string; title: string; artist: string; played_at?: string }>>>(new Map());
	let loadingStationTracks = $state<string | null>(null);

	// Library sync state
	let libraryStats = $state<{
		total_tracks: number;
		total_ai_analyzed: number;
		computed_at: string | null;
	} | null>(null);
	let syncing = $state(false);
	let syncError = $state<string | null>(null);
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

	onMount(async () => {
		if (!authStore.isAdmin) {
			goto('/');
			return;
		}

		await Promise.all([loadStations(), loadAiCapabilities(), loadLibraryStats()]);
	});

	async function loadLibraryStats() {
		try {
			libraryStats = await api.getLibraryStats();
		} catch (e) {
			console.error('Failed to load library stats:', e);
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

		// Cancel any existing curation
		if (curationAbort) {
			curationAbort();
		}

		analyzingDescription = true;
		aiResult = null;
		createError = null;
		showFullPlaylist = false;
		curationProgress = null;
		curationComplete = false;

		// Store track IDs as they come in
		let finalTrackIds: string[] = [];

		curationAbort = api.curateWithProgress(
			description,
			200,
			// onProgress
			(progress) => {
				curationProgress = progress;
				// Check if we received the final completion with track IDs
				if (progress.step === 'completed' && progress.reasoning) {
					try {
						const result = JSON.parse(progress.reasoning);
						if (result.track_ids) {
							finalTrackIds = result.track_ids;
						}
					} catch {
						// Not JSON, that's fine
					}
				}
			},
			// onComplete
			async (ids) => {
				// Use the track IDs from progress if available, otherwise from onComplete
				const trackIds = finalTrackIds.length > 0 ? finalTrackIds : ids;

				// Show completion state briefly before transitioning
				curationComplete = true;
				curationProgress = {
					step: 'completed',
					message: `Found ${trackIds.length} matching tracks! Loading details...`,
					tracks_selected: trackIds.length
				};

				// Fetch track details
				let trackDetails: Array<{ id: string; title: string; artist: string }> = [];
				if (trackIds.length > 0) {
					try {
						const result = await api.getTracksByIds(trackIds);
						trackDetails = result.tracks;
					} catch (e) {
						console.error('Failed to fetch track details:', e);
						// Fall back to placeholder
						trackDetails = trackIds.map(id => ({ id, title: 'Unknown', artist: 'Unknown' }));
					}
				}

				// Update progress to show we're done
				curationProgress = {
					step: 'completed',
					message: `Found ${trackIds.length} matching tracks!`,
					tracks_selected: trackIds.length
				};

				// Delay clearing progress to show completion state
				setTimeout(() => {
					analyzingDescription = false;
					curationAbort = null;

					if (trackIds.length > 0) {
						// Extract genres from the description
						const defaultGenres = description.split(/[,\s]+/).slice(0, 3);
						genresInput = defaultGenres.join(', ');

						aiResult = {
							tracks_found: trackIds.length,
							tracks: trackDetails.length > 0 ? trackDetails : trackIds.map(id => ({ id, title: 'Track', artist: '' })),
							sample_tracks: trackDetails.slice(0, 5).map(t => `${t.artist} - ${t.title}`)
						};
					}

					// Clear progress after showing result
					setTimeout(() => {
						curationProgress = null;
						curationComplete = false;
					}, 500);
				}, 1000);
			},
			// onError
			(error) => {
				console.error('Curation failed:', error);
				createError = error || 'AI curation failed. Make sure you have synced your library first.';
				analyzingDescription = false;
				curationProgress = null;
				curationAbort = null;
				curationComplete = false;
			}
		);
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
				const result = await api.getStationTracks(stationId, 50);
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
					<div class="text-2xl font-bold text-purple-400">{libraryStats.total_ai_analyzed.toLocaleString()}</div>
					<div class="text-sm text-gray-400">AI Analyzed</div>
				</div>
				<div class="bg-gray-700 rounded p-4">
					<div class="text-sm text-gray-400">Last Sync</div>
					<div class="text-sm font-semibold">{libraryStats.computed_at ? new Date(libraryStats.computed_at).toLocaleString() : 'Never'}</div>
				</div>
			</div>
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
						onblur={() => {
							if (aiAvailable && description.trim() && !genresInput) {
								handleAnalyzeDescription();
							}
						}}
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

					<!-- AI Curation Progress Display -->
					{#if curationProgress}
						{@const steps = ['started', 'checking_cache', 'ai_analyzing_query', 'searching_tracks', 'ai_selecting_tracks', 'completed']}
						{@const stepLabels = ['Starting', 'Cache Check', 'Analyzing Query', 'Searching', 'Selecting Tracks', 'Complete']}
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

									<!-- Thinking/context indicator -->
									{#if curationProgress.thinking}
										<div class="mt-2 text-sm text-purple-300 flex items-start gap-2 bg-purple-900/30 rounded p-2">
											<span>💭</span>
											<span class="italic">{curationProgress.thinking}</span>
										</div>
									{/if}

									<!-- Candidate count -->
									{#if curationProgress.candidate_count}
										<div class="mt-2 text-sm text-blue-300 flex items-center gap-2">
											<span>📊</span>
											<span>Analyzing {curationProgress.candidate_count} candidate tracks...</span>
										</div>
									{/if}

									<!-- Tracks selected count -->
									{#if curationProgress.tracks_selected}
										<div class="mt-2 text-sm text-green-300 font-medium flex items-center gap-2">
											<span>✅</span>
											<span>{curationProgress.tracks_selected} tracks selected</span>
										</div>
									{/if}

									<!-- Filters applied (collapsed by default) -->
									{#if curationProgress.filters_applied}
										<details class="mt-3">
											<summary class="text-xs text-gray-400 cursor-pointer hover:text-gray-300 select-none">
												▶ View applied filters
											</summary>
											<div class="mt-2 text-xs text-gray-500 font-mono bg-gray-800/50 rounded p-2 overflow-x-auto max-h-32 overflow-y-auto">
												<pre class="whitespace-pre-wrap">{JSON.stringify(curationProgress.filters_applied, null, 2)}</pre>
											</div>
										</details>
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
								onclick={() => handleDeleteStation(station.id)}
								class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white text-sm rounded-lg whitespace-nowrap"
							>
								Delete
							</button>
						</div>
					</div>

					<!-- Expandable Tracks Section -->
					{#if expandedStationId === station.id}
						<div class="mt-4 pt-4 border-t border-gray-700">
							<h4 class="text-sm font-semibold text-gray-300 mb-3 flex items-center gap-2">
								<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
									<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"></path>
								</svg>
								Recent Tracks Played
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
												<th class="text-right py-2 px-2">Played</th>
											</tr>
										</thead>
										<tbody>
											{#each stationTracks.get(station.id) || [] as track, i}
												<tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
													<td class="py-2 px-2 text-gray-500">{i + 1}</td>
													<td class="py-2 px-2 text-white font-medium truncate max-w-[200px]" title={track.title}>{track.title}</td>
													<td class="py-2 px-2 text-gray-400 truncate max-w-[150px]" title={track.artist}>{track.artist}</td>
													<td class="py-2 px-2 text-gray-500 truncate max-w-[150px]" title={track.album}>{track.album}</td>
													<td class="py-2 px-2 text-right text-gray-500 text-xs whitespace-nowrap">
														{#if track.played_at}
															{new Date(track.played_at).toLocaleString()}
														{:else}
															-
														{/if}
													</td>
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
