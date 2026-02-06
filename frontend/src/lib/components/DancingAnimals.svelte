<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { DEFAULT_ANIMAL_CONFIGS } from './dancing-animals/types';
	import type { AnimalConfigWithBand } from './dancing-animals/types';

	// Props
	let {
		audioElement,
		isPlaying = false,
		animalConfigs = DEFAULT_ANIMAL_CONFIGS
	}: {
		audioElement?: HTMLAudioElement;
		isPlaying: boolean;
		animalConfigs?: AnimalConfigWithBand[];
	} = $props();

	// State
	let container = $state<HTMLDivElement>();
	let threeScene: import('./dancing-animals/ThreeScene').ThreeScene | null = null;
	let isLoading = $state(true);
	let loadError = $state<string | null>(null);
	let webglSupported = $state(true);

	// Audio connection state
	let audioContext: AudioContext | null = null;
	let analyser: AnalyserNode | null = null;
	let source: MediaElementAudioSourceNode | null = null;
	let isAudioConnected = false;

	onMount(async () => {
		// Check WebGL support
		const canvas = document.createElement('canvas');
		const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
		if (!gl) {
			webglSupported = false;
			loadError = 'WebGL is not supported in this browser';
			isLoading = false;
			return;
		}

		if (!container) return;

		try {
			// Dynamically import Three.js scene (avoids SSR issues)
			const { ThreeScene } = await import('./dancing-animals/ThreeScene');
			threeScene = new ThreeScene(container);

			// Load animal models
			await threeScene.loadAnimals(animalConfigs);

			isLoading = false;
			threeScene.start();
		} catch (error) {
			console.error('Failed to initialize 3D scene:', error);
			loadError = error instanceof Error ? error.message : 'Failed to load 3D scene';
			isLoading = false;
		}
	});

	onDestroy(() => {
		disconnectAudio();
		if (threeScene) {
			threeScene.dispose();
			threeScene = null;
		}
	});

	// Connect audio when playing starts
	$effect(() => {
		if (audioElement && isPlaying && threeScene && !isAudioConnected) {
			connectAudio();
		}
	});

	function connectAudio() {
		if (!audioElement || isAudioConnected) return;

		try {
			// Create audio context
			audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();

			// Create analyser node
			analyser = audioContext.createAnalyser();
			analyser.fftSize = 512; // 256 frequency bins
			analyser.smoothingTimeConstant = 0.7;

			// Connect audio element to analyser
			source = audioContext.createMediaElementSource(audioElement);
			source.connect(analyser);
			analyser.connect(audioContext.destination);

			// Connect to Three.js scene
			if (threeScene && analyser) {
				threeScene.connectAudio(analyser, audioContext.sampleRate);
			}

			isAudioConnected = true;
			console.log('Audio connected to 3D visualizer');
		} catch (error) {
			console.warn('Failed to connect audio:', error);
		}
	}

	function disconnectAudio() {
		if (source) {
			try {
				source.disconnect();
			} catch {
				// Ignore
			}
		}
		if (analyser) {
			try {
				analyser.disconnect();
			} catch {
				// Ignore
			}
		}
		if (audioContext) {
			try {
				audioContext.close();
			} catch {
				// Ignore
			}
		}
		if (threeScene) {
			threeScene.disconnectAudio();
		}

		audioContext = null;
		analyser = null;
		source = null;
		isAudioConnected = false;
	}
</script>

<div bind:this={container} class="w-full h-full relative overflow-hidden">
	{#if isLoading}
		<div
			class="absolute inset-0 flex items-center justify-center bg-gray-900"
		>
			<div class="text-center">
				<div
					class="animate-spin rounded-full h-10 w-10 border-b-2 border-blue-500 mx-auto mb-3"
				></div>
				<p class="text-gray-400 text-sm">Loading animals...</p>
			</div>
		</div>
	{/if}

	{#if loadError}
		<div
			class="absolute inset-0 flex items-center justify-center bg-gray-900"
		>
			<div class="text-center p-4">
				<p class="text-red-400 text-sm mb-2">Failed to load 3D scene</p>
				<p class="text-gray-500 text-xs">{loadError}</p>
			</div>
		</div>
	{/if}
</div>

<style>
	div :global(canvas) {
		display: block;
		width: 100%;
		height: 100%;
	}
</style>
