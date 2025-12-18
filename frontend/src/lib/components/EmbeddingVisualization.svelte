<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api, type EmbeddingPoint } from '$lib/api/client';

	// State
	let loading = $state(true);
	let error = $state<string | null>(null);
	let points = $state<EmbeddingPoint[]>([]);
	let cacheRebuilt = $state(false);
	let plotContainer: HTMLDivElement | undefined = $state();
	let Plotly: typeof import('plotly.js-dist-min') | null = $state(null);

	// Genre color mapping with distinct colors
	const genreColors: Record<string, string> = {
		'Rap': '#e6194b',
		'Hip Hop': '#e6194b',
		'Hip-Hop': '#e6194b',
		'Alternative': '#3cb44b',
		'Alternative Rock': '#3cb44b',
		'Indie Rock': '#3cb44b',
		'Rock': '#4363d8',
		'Progressive Rock': '#4363d8',
		'Pop': '#f58231',
		'R&B': '#911eb4',
		'R&B/Soul': '#911eb4',
		'Jazz': '#46f0f0',
		'Electronic': '#f032e6',
		'Electro': '#f032e6',
		'Dance': '#f032e6',
		'Country': '#bcf60c',
		'Metal': '#fabebe',
		'Screwed': '#008080',
		'Latin Music': '#e6beff',
		'Salsa': '#e6beff',
		'Brazilian Music': '#9a6324',
		'Asian Music': '#fffac8',
		'Films/Games': '#800000',
		'Films': '#800000',
		'Shoegaze': '#aaffc3',
		'New Wave': '#808000',
		'Unknown': '#808080',
	};

	const defaultColor = '#808080';

	function getGenreColor(genre: string | null): string {
		if (!genre) return defaultColor;
		return genreColors[genre] || defaultColor;
	}

	function categorizeGenre(genre: string | null): string {
		if (!genre) return 'Unknown';
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

	async function loadData() {
		loading = true;
		error = null;

		try {
			// Fetch pre-computed 2D coordinates from server
			const response = await api.getEmbeddingsForVisualization();
			points = response.points;
			cacheRebuilt = response.cache_rebuilt;

			if (points.length === 0) {
				error = 'No embeddings found. Generate audio embeddings first.';
				loading = false;
				return;
			}

			// Render the plot (no client-side dimension reduction needed!)
			await renderPlot();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load embeddings';
		} finally {
			loading = false;
		}
	}

	async function renderPlot() {
		if (!plotContainer || !Plotly) return;

		// Group points by category for separate traces (enables legend)
		const categorizedPoints: Record<string, { x: number[], y: number[], text: string[] }> = {};

		for (const p of points) {
			const category = categorizeGenre(p.genre);

			if (!categorizedPoints[category]) {
				categorizedPoints[category] = { x: [], y: [], text: [] };
			}

			// Use pre-computed x/y from server
			categorizedPoints[category].x.push(p.x);
			categorizedPoints[category].y.push(p.y);
			categorizedPoints[category].text.push(`${p.title}<br>${p.artist}<br>${p.album}<br><b>${p.genre || 'Unknown'}</b>`);
		}

		// Create traces for each category
		const traces = Object.entries(categorizedPoints).map(([category, data]) => ({
			x: data.x,
			y: data.y,
			mode: 'markers' as const,
			type: 'scatter' as const,
			name: category,
			marker: {
				size: 6,
				color: categoryColors[category] || defaultColor,
				opacity: 0.8,
			},
			text: data.text,
			hoverinfo: 'text' as const,
			hoverlabel: {
				bgcolor: '#1f2937',
				bordercolor: categoryColors[category] || defaultColor,
				font: { color: 'white', size: 12 }
			}
		}));

		// Sort traces by count (largest first for better legend ordering)
		traces.sort((a, b) => b.x.length - a.x.length);

		const layout = {
			title: {
				text: `Audio Embeddings (${points.length} tracks)`,
				font: { color: '#e5e7eb', size: 16 }
			},
			paper_bgcolor: '#111827',
			plot_bgcolor: '#1f2937',
			xaxis: {
				title: 'PCA-1',
				color: '#9ca3af',
				gridcolor: '#374151',
				zerolinecolor: '#4b5563'
			},
			yaxis: {
				title: 'PCA-2',
				color: '#9ca3af',
				gridcolor: '#374151',
				zerolinecolor: '#4b5563'
			},
			margin: { l: 50, r: 20, t: 50, b: 50 },
			hovermode: 'closest' as const,
			legend: {
				font: { color: '#e5e7eb', size: 11 },
				bgcolor: 'rgba(31, 41, 55, 0.8)',
				bordercolor: '#4b5563',
				borderwidth: 1,
				x: 1,
				xanchor: 'right' as const,
				y: 1,
			},
			showlegend: true,
		};

		const config = {
			responsive: true,
			displayModeBar: true,
			modeBarButtonsToRemove: ['lasso2d', 'select2d'] as any[],
			displaylogo: false,
		};

		await Plotly.newPlot(plotContainer, traces, layout, config);
	}

	onMount(async () => {
		// Dynamically import Plotly to avoid SSR issues
		Plotly = await import('plotly.js-dist-min');
		await loadData();
	});

	onDestroy(() => {
		if (plotContainer && Plotly) {
			Plotly.purge(plotContainer);
		}
	});

	// Re-render when container or Plotly changes
	$effect(() => {
		if (plotContainer && Plotly && points.length > 0) {
			renderPlot();
		}
	});
</script>

<div class="bg-gray-800 rounded-lg p-4">
	<div class="flex items-center justify-between mb-4">
		<h3 class="text-lg font-semibold text-white flex items-center gap-2">
			<svg class="w-5 h-5 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"></path>
			</svg>
			Embedding Visualization
		</h3>
		<button
			onclick={loadData}
			disabled={loading}
			class="flex items-center gap-2 px-3 py-1.5 bg-purple-600 hover:bg-purple-700 disabled:bg-gray-600 text-white text-sm rounded-lg transition-colors"
		>
			{#if loading}
				<svg class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
					<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
					<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
				</svg>
				Loading...
			{:else}
				<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path>
				</svg>
				Refresh
			{/if}
		</button>
	</div>

	{#if error}
		<div class="bg-red-900/30 border border-red-600/50 rounded-lg p-4 text-red-200 text-sm">
			{error}
		</div>
	{:else if loading}
		<div class="flex flex-col items-center justify-center py-16 gap-4">
			<div class="relative">
				<svg class="w-16 h-16 text-purple-400 animate-pulse" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"></path>
				</svg>
				<div class="absolute inset-0 rounded-full border-4 border-purple-500/30 animate-ping"></div>
			</div>
			<div class="text-gray-400 text-center">
				<p class="font-medium">Loading visualization data...</p>
				<p class="text-sm mt-1">Pre-computed coordinates load quickly</p>
			</div>
		</div>
	{:else}
		<div
			bind:this={plotContainer}
			class="w-full h-[500px] rounded-lg overflow-hidden"
		></div>
		<p class="text-xs text-gray-500 mt-2 text-center">
			Hover over points to see track details. Colors represent genres. Click legend items to toggle visibility. Nearby points indicate similar audio characteristics.
		</p>
	{/if}
</div>
