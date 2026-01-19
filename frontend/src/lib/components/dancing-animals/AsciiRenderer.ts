import * as THREE from 'three';

/**
 * ASCII post-processing renderer for 3D scenes.
 * Converts the rendered scene to ASCII characters with glow effects.
 */
export class AsciiRenderer {
	private renderer: THREE.WebGLRenderer;
	private renderTarget: THREE.WebGLRenderTarget;
	private canvas2D: HTMLCanvasElement;
	private ctx: CanvasRenderingContext2D;
	private pixelBuffer: Uint8Array;

	// ASCII settings - smaller cells for higher detail (catches eyes, small features)
	private readonly ASCII_CHARS = ' .:-=+*#%@';
	private cellWidth = 4;
	private cellHeight = 6;
	private fontSize = 5;
	private fontFamily = 'monospace';

	// Colors
	private glowColor = '#60a5fa'; // blue-400
	private textColor = '#f0f0f0';
	private bgColor = 'transparent';

	// Glow settings
	private glowIntensity = 0.8;
	private glowSize = 8;

	// Resolution
	private cols = 0;
	private rows = 0;

	// Waveform animation
	private wavePhase = 0;
	private readonly WAVE_CHARS = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

	constructor(renderer: THREE.WebGLRenderer, container: HTMLElement) {
		this.renderer = renderer;

		// Create render target for capturing the 3D scene
		const size = renderer.getSize(new THREE.Vector2());
		this.renderTarget = new THREE.WebGLRenderTarget(size.x, size.y);

		// Create 2D canvas overlay for ASCII rendering
		this.canvas2D = document.createElement('canvas');
		this.canvas2D.style.position = 'absolute';
		this.canvas2D.style.top = '0';
		this.canvas2D.style.left = '0';
		this.canvas2D.style.width = '100%';
		this.canvas2D.style.height = '100%';
		this.canvas2D.style.pointerEvents = 'none';
		container.appendChild(this.canvas2D);

		const ctx = this.canvas2D.getContext('2d');
		if (!ctx) throw new Error('Could not get 2D context');
		this.ctx = ctx;

		// Buffer for reading pixels
		this.pixelBuffer = new Uint8Array(0);

		this.resize();
	}

	/**
	 * Handle resize
	 */
	resize(): void {
		const size = this.renderer.getSize(new THREE.Vector2());
		const width = size.x;
		const height = size.y;

		// Update render target
		this.renderTarget.setSize(width, height);

		// Update 2D canvas
		this.canvas2D.width = width;
		this.canvas2D.height = height;

		// Calculate grid dimensions
		this.cols = Math.floor(width / this.cellWidth);
		this.rows = Math.floor(height / this.cellHeight);

		// Resize pixel buffer
		this.pixelBuffer = new Uint8Array(width * height * 4);
	}

	// Debug mode - set to true to see raw 3D render
	private debugMode = false;

	/**
	 * Render scene as ASCII
	 */
	render(scene: THREE.Scene, camera: THREE.Camera, audioBands?: { bass: number; mids: number; highs: number }): void {
		const width = this.renderTarget.width;
		const height = this.renderTarget.height;

		// Render to off-screen target only (not to screen)
		this.renderer.setRenderTarget(this.renderTarget);
		this.renderer.render(scene, camera);
		this.renderer.setRenderTarget(null);

		// Debug: show raw 3D render instead of ASCII
		if (this.debugMode) {
			this.renderer.render(scene, camera);
			return;
		}

		// Clear the WebGL canvas (hide 3D model)
		this.renderer.clear();

		// Read pixels from render target
		this.renderer.readRenderTargetPixels(
			this.renderTarget,
			0, 0, width, height,
			this.pixelBuffer
		);

		// Fill 2D canvas with solid background
		this.ctx.fillStyle = '#111827'; // gray-900
		this.ctx.fillRect(0, 0, this.canvas2D.width, this.canvas2D.height);

		// Set up text rendering
		this.ctx.font = `${this.fontSize}px ${this.fontFamily}`;
		this.ctx.textBaseline = 'top';

		// Subtle, stable glow - no audio reactivity for consistent colors
		this.ctx.shadowColor = this.glowColor;
		this.ctx.shadowBlur = this.glowSize * 0.5; // Reduced, constant glow

		// Sample and render ASCII characters
		for (let row = 0; row < this.rows; row++) {
			for (let col = 0; col < this.cols; col++) {
				// Multi-point sampling to catch small features like eyes
				// Sample 5 points: center + 4 corners
				const samples = [
					[0.5, 0.5], // center
					[0.2, 0.2], // top-left
					[0.8, 0.2], // top-right
					[0.2, 0.8], // bottom-left
					[0.8, 0.8]  // bottom-right
				];

				let bestR = 0, bestG = 0, bestB = 0, bestA = 0;

				for (const [ox, oy] of samples) {
					const sampleX = Math.floor((col + ox) * this.cellWidth);
					const sampleY = Math.floor((row + oy) * this.cellHeight);

					// Flip Y because WebGL has origin at bottom-left
					const flippedY = height - 1 - sampleY;
					const pixelIndex = (flippedY * width + sampleX) * 4;

					const r = this.pixelBuffer[pixelIndex];
					const g = this.pixelBuffer[pixelIndex + 1];
					const b = this.pixelBuffer[pixelIndex + 2];
					const a = this.pixelBuffer[pixelIndex + 3];

					// Keep the sample with highest alpha (most opaque)
					if (a > bestA) {
						bestR = r;
						bestG = g;
						bestB = b;
						bestA = a;
					}
				}

				// Skip fully transparent pixels (background)
				// Note: using threshold of 1 to catch any pixel with any opacity
				if (bestA < 1) {
					// Debug: log if we're skipping pixels with low but non-zero alpha
					// This might catch eyes with transparency
					continue;
				}

				// Calculate brightness (0-1)
				const brightness = (bestR * 0.299 + bestG * 0.587 + bestB * 0.114) / 255;

				// Map brightness to character density (brighter = denser)
				const charIndex = Math.min(
					this.ASCII_CHARS.length - 1,
					Math.max(1, Math.floor(brightness * (this.ASCII_CHARS.length - 1)))
				);
				const char = this.ASCII_CHARS[charIndex];

				// Boost colors to be visible while preserving accurate hues
				// Use luminance-based boost to maintain color balance
				const luminance = bestR * 0.299 + bestG * 0.587 + bestB * 0.114;
				const targetLuminance = 180;

				let finalR = bestR, finalG = bestG, finalB = bestB;

				if (luminance > 0 && luminance < targetLuminance) {
					// Boost based on luminance to preserve color ratios
					const boost = targetLuminance / luminance;
					finalR = Math.min(255, Math.round(bestR * boost));
					finalG = Math.min(255, Math.round(bestG * boost));
					finalB = Math.min(255, Math.round(bestB * boost));
				} else if (luminance === 0) {
					// Pure black - make it visible gray
					finalR = finalG = finalB = 160;
				}

				// Gentle minimum to ensure visibility without color shift
				const minChannel = 80;
				finalR = Math.max(finalR, minChannel);
				finalG = Math.max(finalG, minChannel);
				finalB = Math.max(finalB, minChannel);

				this.ctx.fillStyle = `rgb(${finalR}, ${finalG}, ${finalB})`;

				// Draw character
				const x = col * this.cellWidth;
				const y = row * this.cellHeight;
				this.ctx.fillText(char, x, y);
			}
		}

		// Draw waveform at bottom
		if (audioBands) {
			this.drawWaveform(audioBands);
		}
	}

	/**
	 * Draw animated waveform at bottom of canvas
	 */
	private drawWaveform(bands: { bass: number; mids: number; highs: number }): void {
		const width = this.canvas2D.width;
		const height = this.canvas2D.height;
		const waveY = height - 15; // Position at very bottom
		const sectionWidth = Math.floor(width / 3);

		this.wavePhase += 0.15;

		this.ctx.font = '14px monospace';
		this.ctx.textBaseline = 'bottom';
		this.ctx.shadowBlur = 10;

		// Bass section (left) - red/pink
		this.ctx.shadowColor = '#ff6b6b';
		this.ctx.fillStyle = '#ff6b6b';
		this.drawWaveSection(0, sectionWidth, waveY, bands.bass, 0.15, 0.4);

		// Mids section (center) - cyan
		this.ctx.shadowColor = '#4ecdc4';
		this.ctx.fillStyle = '#4ecdc4';
		this.drawWaveSection(sectionWidth, sectionWidth, waveY, bands.mids, 0.25, 0.3);

		// Highs section (right) - yellow
		this.ctx.shadowColor = '#ffe66d';
		this.ctx.fillStyle = '#ffe66d';
		this.drawWaveSection(sectionWidth * 2, sectionWidth, waveY, bands.highs, 0.5, 0.2);

		// Reset shadow
		this.ctx.shadowBlur = 0;
	}

	/**
	 * Draw a single waveform section
	 */
	private drawWaveSection(startX: number, width: number, y: number, level: number, speed: number, waveAmp: number): void {
		const charWidth = 8;
		const numChars = Math.floor(width / charWidth);

		for (let i = 0; i < numChars; i++) {
			const x = startX + i * charWidth;
			const phase = this.wavePhase * speed + i * waveAmp;
			const baseHeight = Math.sin(phase) * 0.5 + 0.5;
			const levelHeight = baseHeight * 0.2 + level * 0.8;
			const charIndex = Math.min(Math.floor(levelHeight * 8), 7);
			this.ctx.fillText(this.WAVE_CHARS[charIndex], x, y);
		}
	}

	/**
	 * Set ASCII density (smaller = more detail)
	 */
	setDensity(cellWidth: number, cellHeight: number, fontSize: number): void {
		this.cellWidth = cellWidth;
		this.cellHeight = cellHeight;
		this.fontSize = fontSize;
		this.resize();
	}

	/**
	 * Set glow color
	 */
	setGlowColor(color: string): void {
		this.glowColor = color;
	}

	/**
	 * Set glow intensity
	 */
	setGlowIntensity(intensity: number): void {
		this.glowIntensity = intensity;
	}

	/**
	 * Clean up resources
	 */
	dispose(): void {
		this.renderTarget.dispose();
		if (this.canvas2D.parentElement) {
			this.canvas2D.parentElement.removeChild(this.canvas2D);
		}
	}
}
