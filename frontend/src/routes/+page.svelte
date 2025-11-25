<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { Station } from '$lib/types';

	let stations = $state<Station[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			stations = await api.getStations();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load stations';
		} finally {
			loading = false;
		}
	});
</script>

<div class="container mx-auto px-4 py-6 md:py-12">
	<h1 class="text-3xl md:text-5xl font-bold mb-6 md:mb-10 text-center text-white">
		Radio Stations
	</h1>

	{#if loading}
		<div class="flex items-center justify-center min-h-[50vh]">
			<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
		</div>
	{:else if error}
		<div class="text-center text-red-500 p-8">
			<p>{error}</p>
		</div>
	{:else if stations.length === 0}
		<div class="text-center text-gray-400 p-8">
			<p class="text-xl mb-4">No stations available</p>
			<p>Check back later or ask an admin to create some stations!</p>
		</div>
	{:else}
		<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 md:gap-6">
			{#each stations as station}
				<a
					href="/station/{station.path}"
					class="group block bg-gray-800 rounded-lg overflow-hidden shadow-lg hover:shadow-2xl transition-all duration-300 hover:scale-105 active:scale-100"
				>
					<div class="relative">
						<div
							class="aspect-square bg-gradient-to-br from-blue-600 to-purple-600 flex items-center justify-center"
						>
							<svg
								class="w-16 h-16 md:w-20 md:h-20 text-white"
								fill="currentColor"
								viewBox="0 0 20 20"
							>
								<path
									d="M18 3a1 1 0 00-1.196-.98l-10 2A1 1 0 006 5v9.114A4.369 4.369 0 005 14c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V7.82l8-1.6v5.894A4.37 4.37 0 0015 12c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V3z"
								/>
							</svg>
						</div>
						{#if station.active}
							<div
								class="absolute top-2 right-2 flex items-center gap-1 bg-green-500 text-white px-2 py-1 rounded-full text-xs md:text-sm font-semibold"
							>
								<span class="w-2 h-2 bg-white rounded-full animate-pulse"></span>
								Live
							</div>
						{/if}
					</div>

					<div class="p-4 md:p-6">
						<h3
							class="text-lg md:text-xl font-bold mb-2 group-hover:text-blue-400 transition-colors truncate"
						>
							{station.name}
						</h3>
						<p class="text-sm md:text-base text-gray-400 mb-3 line-clamp-2">
							{station.description}
						</p>

						<div class="flex flex-wrap gap-1.5">
							{#each station.genres.slice(0, 3) as genre}
								<span class="px-2 py-1 bg-gray-700 rounded text-xs md:text-sm text-gray-300">
									{genre}
								</span>
							{/each}
							{#if station.genres.length > 3}
								<span class="px-2 py-1 bg-gray-700 rounded text-xs md:text-sm text-gray-300">
									+{station.genres.length - 3}
								</span>
							{/if}
						</div>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>

<style>
	.line-clamp-2 {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
</style>
