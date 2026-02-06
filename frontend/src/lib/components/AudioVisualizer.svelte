<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	// Props
	let {
		audioElement,
		isPlaying = false,
		variant = 'bars'
	}: {
		audioElement?: HTMLAudioElement;
		isPlaying: boolean;
		variant?: 'bars' | 'waveform' | 'circular';
	} = $props();

	// Canvas and animation state
	let canvas = $state<HTMLCanvasElement>();
	let animationFrame: number;

	// Web Audio API state
	let audioContext: AudioContext | null = null;
	let analyser: AnalyserNode | null = null;
	let source: MediaElementAudioSourceNode | null = null;
	let dataArray: Uint8Array<ArrayBuffer> | null = null;
	let isConnected = false;

	// Smoothed visualization data
	let smoothedData: number[] = [];
	const smoothingFactor = 0.25;

	onMount(() => {
		startAnimation();
		handleResize();
	});

	onDestroy(() => {
		if (animationFrame) {
			cancelAnimationFrame(animationFrame);
		}
		disconnectAudio();
	});

	$effect(() => {
		if (audioElement && isPlaying && !isConnected) {
			connectAudio();
		}
	});

	function connectAudio() {
		if (!audioElement || isConnected) return;

		try {
			// Create audio context
			audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();

			// Create analyser node
			analyser = audioContext.createAnalyser();
			analyser.fftSize = 256;
			analyser.smoothingTimeConstant = 0.8;

			// Connect audio element to analyser
			source = audioContext.createMediaElementSource(audioElement);
			source.connect(analyser);
			analyser.connect(audioContext.destination);

			// Create data array for frequency data
			dataArray = new Uint8Array(analyser.frequencyBinCount);
			smoothedData = new Array(analyser.frequencyBinCount).fill(0);

			isConnected = true;
			console.log('Audio visualizer connected');
		} catch (e) {
			console.warn('Failed to connect audio visualizer:', e);
		}
	}

	function disconnectAudio() {
		if (source) {
			try {
				source.disconnect();
			} catch (e) {
				// Ignore
			}
		}
		if (analyser) {
			try {
				analyser.disconnect();
			} catch (e) {
				// Ignore
			}
		}
		if (audioContext) {
			try {
				audioContext.close();
			} catch (e) {
				// Ignore
			}
		}
		audioContext = null;
		analyser = null;
		source = null;
		dataArray = null;
		isConnected = false;
	}

	function startAnimation() {
		const draw = () => {
			animationFrame = requestAnimationFrame(draw);

			if (!canvas) return;

			const ctx = canvas.getContext('2d');
			if (!ctx) return;

			const width = canvas.width;
			const height = canvas.height;

			// Get frequency data if available
			if (analyser && dataArray && isPlaying) {
				analyser.getByteFrequencyData(dataArray);

				// Smooth the data
				for (let i = 0; i < dataArray.length; i++) {
					const normalized = dataArray[i] / 255;
					smoothedData[i] = smoothedData[i] * (1 - smoothingFactor) + normalized * smoothingFactor;
				}
			} else {
				// Decay when not playing
				for (let i = 0; i < smoothedData.length; i++) {
					smoothedData[i] *= 0.95;
				}
			}

			// Clear canvas
			ctx.fillStyle = 'rgba(17, 24, 39, 0.95)';
			ctx.fillRect(0, 0, width, height);

			if (variant === 'bars') {
				drawBars(ctx, width, height);
			} else if (variant === 'waveform') {
				drawWaveform(ctx, width, height);
			} else if (variant === 'circular') {
				drawCircular(ctx, width, height);
			}
		};

		animationFrame = requestAnimationFrame(draw);
	}

	function drawBars(ctx: CanvasRenderingContext2D, width: number, height: number) {
		const barCount = Math.min(smoothedData.length, 64);
		const barWidth = width / barCount - 2;
		const maxBarHeight = height * 0.85;

		for (let i = 0; i < barCount; i++) {
			const value = smoothedData[i] || 0;
			const barHeight = value * maxBarHeight;
			const x = i * (barWidth + 2) + 1;
			const y = height - barHeight;

			// Create gradient
			const gradient = ctx.createLinearGradient(x, height, x, y);
			const hue = 220 + (i / barCount) * 80;
			gradient.addColorStop(0, `hsla(${hue}, 80%, 60%, 0.9)`);
			gradient.addColorStop(1, `hsla(${hue + 40}, 90%, 70%, 0.9)`);

			ctx.fillStyle = gradient;

			// Rounded top
			const radius = Math.min(barWidth / 2, 4);
			ctx.beginPath();
			ctx.moveTo(x, height);
			ctx.lineTo(x, y + radius);
			ctx.quadraticCurveTo(x, y, x + radius, y);
			ctx.lineTo(x + barWidth - radius, y);
			ctx.quadraticCurveTo(x + barWidth, y, x + barWidth, y + radius);
			ctx.lineTo(x + barWidth, height);
			ctx.closePath();
			ctx.fill();

			// Glow for loud bars
			if (value > 0.5) {
				ctx.shadowColor = `hsla(${hue}, 80%, 60%, 0.5)`;
				ctx.shadowBlur = 15;
				ctx.fill();
				ctx.shadowBlur = 0;
			}
		}

		// Reflection effect
		ctx.save();
		ctx.globalAlpha = 0.15;
		ctx.scale(1, -0.3);
		ctx.translate(0, -height * 4.3);

		for (let i = 0; i < barCount; i++) {
			const value = smoothedData[i] || 0;
			const barHeight = value * maxBarHeight;
			const x = i * (barWidth + 2) + 1;
			const y = height - barHeight;

			const hue = 220 + (i / barCount) * 80;
			ctx.fillStyle = `hsla(${hue}, 80%, 60%, 0.5)`;
			ctx.fillRect(x, y, barWidth, barHeight);
		}

		ctx.restore();
	}

	function drawWaveform(ctx: CanvasRenderingContext2D, width: number, height: number) {
		const midY = height / 2;

		ctx.beginPath();
		ctx.moveTo(0, midY);

		const pointCount = smoothedData.length;
		for (let i = 0; i < pointCount; i++) {
			const x = (i / pointCount) * width;
			const amplitude = (smoothedData[i] || 0) * height * 0.4;
			const y = midY + Math.sin((i / pointCount) * Math.PI * 4) * amplitude;
			ctx.lineTo(x, y);
		}

		ctx.strokeStyle = 'rgba(59, 130, 246, 0.8)';
		ctx.lineWidth = 2;
		ctx.stroke();

		// Mirror
		ctx.beginPath();
		ctx.moveTo(0, midY);

		for (let i = 0; i < pointCount; i++) {
			const x = (i / pointCount) * width;
			const amplitude = (smoothedData[i] || 0) * height * 0.4;
			const y = midY - Math.sin((i / pointCount) * Math.PI * 4) * amplitude;
			ctx.lineTo(x, y);
		}

		ctx.strokeStyle = 'rgba(139, 92, 246, 0.8)';
		ctx.stroke();
	}

	function drawCircular(ctx: CanvasRenderingContext2D, width: number, height: number) {
		const centerX = width / 2;
		const centerY = height / 2;
		const baseRadius = Math.min(width, height) * 0.25;

		const barCount = Math.min(smoothedData.length, 64);

		for (let i = 0; i < barCount; i++) {
			const value = smoothedData[i] || 0;
			const angle = (i / barCount) * Math.PI * 2 - Math.PI / 2;
			const barLength = value * baseRadius * 0.8;

			const x1 = centerX + Math.cos(angle) * baseRadius;
			const y1 = centerY + Math.sin(angle) * baseRadius;
			const x2 = centerX + Math.cos(angle) * (baseRadius + barLength);
			const y2 = centerY + Math.sin(angle) * (baseRadius + barLength);

			const hue = 220 + (i / barCount) * 80;
			ctx.strokeStyle = `hsla(${hue}, 80%, ${50 + value * 30}%, ${0.5 + value * 0.5})`;
			ctx.lineWidth = 3;
			ctx.beginPath();
			ctx.moveTo(x1, y1);
			ctx.lineTo(x2, y2);
			ctx.stroke();
		}

		// Center circle
		ctx.beginPath();
		ctx.arc(centerX, centerY, baseRadius * 0.3, 0, Math.PI * 2);
		ctx.fillStyle = 'rgba(59, 130, 246, 0.3)';
		ctx.fill();
	}

	function handleResize() {
		if (canvas) {
			const rect = canvas.getBoundingClientRect();
			canvas.width = rect.width * window.devicePixelRatio;
			canvas.height = rect.height * window.devicePixelRatio;
		}
	}
</script>

<svelte:window onresize={handleResize} />

<canvas
	bind:this={canvas}
	class="w-full h-full rounded-lg"
></canvas>

<style>
	canvas {
		background: rgb(17, 24, 39);
	}
</style>
