<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
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
	let aiResult = $state<{ tracks_found: number; sample_tracks: string[] } | null>(null);

	onMount(async () => {
		if (!authStore.isAdmin) {
			goto('/');
			return;
		}

		await Promise.all([loadStations(), loadAiCapabilities()]);
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
		try {
			const result = await api.analyzeDescription(description);
			genresInput = result.genres.join(', ');
			aiResult = {
				tracks_found: result.tracks_found,
				sample_tracks: result.sample_tracks
			};
		} catch (e) {
			console.error('Failed to analyze description:', e);
			createError =
				e instanceof Error
					? e.message
					: 'AI analysis failed. Please check your API key and try again.';
		} finally {
			analyzingDescription = false;
		}
	}

	async function handleCreateStation(e: Event) {
		e.preventDefault();
		creating = true;
		createError = null;

		try {
			const genres = genresInput.split(',').map((g) => g.trim()).filter(Boolean);

			await api.createStation({
				path: path.toLowerCase().replace(/\s+/g, '-'),
				name,
				description,
				genres
			});

			// Reset form
			path = '';
			name = '';
			description = '';
			genresInput = '';
			showCreateForm = false;
			useAI = false;

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
				{/if}

				{#if aiResult}
					<div class="mb-4 bg-gradient-to-r from-purple-900/40 to-blue-900/40 border border-purple-500/50 rounded-lg p-4">
						<div class="flex items-center gap-2 mb-2">
							<svg class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
							</svg>
							<span class="font-semibold text-green-400">
								Found {aiResult.tracks_found} matching tracks in your library!
							</span>
						</div>
						{#if aiResult.sample_tracks.length > 0}
							<div class="mt-2">
								<p class="text-sm text-gray-300 mb-1">Sample tracks:</p>
								<ul class="text-sm text-gray-400 space-y-1">
									{#each aiResult.sample_tracks as track}
										<li class="flex items-start gap-2">
											<span class="text-purple-400">♪</span>
											<span>{track}</span>
										</li>
									{/each}
								</ul>
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
								onclick={() => handleDeleteStation(station.id)}
								class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white text-sm rounded-lg whitespace-nowrap"
							>
								Delete
							</button>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
