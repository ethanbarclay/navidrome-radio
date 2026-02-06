import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { AnimalController } from './AnimalController';
import { AudioAnalyzer } from './AudioAnalyzer';
import { AsciiRenderer } from './AsciiRenderer';
import type { AnimalConfigWithBand, AudioBands, BeatState } from './types';

/**
 * Manages the Three.js scene with dancing animals.
 */
export class ThreeScene {
	private container: HTMLElement;
	private renderer: THREE.WebGLRenderer;
	private scene: THREE.Scene;
	private camera: THREE.PerspectiveCamera;
	private clock: THREE.Clock;
	private animationFrameId: number | null = null;

	private loader: GLTFLoader;
	private animals: AnimalController[] = [];
	private audioAnalyzer: AudioAnalyzer | null = null;

	// ASCII rendering
	private asciiRenderer: AsciiRenderer | null = null;
	private asciiEnabled = true;

	private isPlaying = false;

	// Bound event handler (stored for proper cleanup)
	private boundHandleResize: () => void;

	constructor(container: HTMLElement) {
		this.container = container;
		this.clock = new THREE.Clock();

		// Store bound handler for proper cleanup
		this.boundHandleResize = this.handleResize.bind(this);

		// Create renderer
		this.renderer = new THREE.WebGLRenderer({
			antialias: true,
			alpha: true,
			powerPreference: 'high-performance'
		});
		this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
		this.renderer.outputColorSpace = THREE.SRGBColorSpace;
		this.renderer.shadowMap.enabled = false; // Disable for performance
		container.appendChild(this.renderer.domElement);

		// Create scene with no background (transparent for ASCII detection)
		this.scene = new THREE.Scene();
		this.scene.background = null; // Transparent - allows ASCII renderer to detect model pixels

		// Create camera (positioned to see all 3 animals)
		this.camera = new THREE.PerspectiveCamera(45, 1, 0.1, 100);
		this.camera.position.set(0, 1.5, 8);
		this.camera.lookAt(0, 0.5, 0);

		// Add lighting
		this.setupLighting();

		// Create GLTF loader
		this.loader = new GLTFLoader();

		// Initialize ASCII renderer
		if (this.asciiEnabled) {
			this.asciiRenderer = new AsciiRenderer(this.renderer, container);
		}

		// Handle resize
		this.handleResize();
		window.addEventListener('resize', this.boundHandleResize);
	}

	/**
	 * Setup scene lighting
	 */
	private setupLighting(): void {
		// Strong ambient light for overall illumination (ensures all parts visible)
		const ambient = new THREE.AmbientLight(0xffffff, 1.0);
		this.scene.add(ambient);

		// Main directional light (sun-like)
		const directional = new THREE.DirectionalLight(0xffffff, 1.0);
		directional.position.set(5, 10, 5);
		this.scene.add(directional);

		// Front light - illuminates faces/eyes directly
		const front = new THREE.DirectionalLight(0xffffff, 0.8);
		front.position.set(0, 2, 10);
		this.scene.add(front);

		// Fill light from the side
		const fill = new THREE.DirectionalLight(0xffffff, 0.5);
		fill.position.set(-5, 3, 0);
		this.scene.add(fill);

		// Rim light from behind
		const rim = new THREE.DirectionalLight(0xffffff, 0.3);
		rim.position.set(0, 2, -5);
		this.scene.add(rim);
	}

	/**
	 * Handle container resize
	 */
	private handleResize(): void {
		const rect = this.container.getBoundingClientRect();
		const width = rect.width;
		const height = rect.height;

		this.camera.aspect = width / height;

		// Adjust camera based on screen width (mobile detection)
		const isMobile = window.innerWidth < 768;
		const isTablet = window.innerWidth < 1000;

		if (isMobile) {
			// Mobile: move camera back
			this.baseCameraZ = 10;
			this.camera.position.set(0, 1.5, this.baseCameraZ);
			this.camera.lookAt(0, 0, 0);
		} else if (isTablet) {
			// Tablet: move camera back moderately
			this.baseCameraZ = 14;
			this.camera.position.set(0, 2, this.baseCameraZ);
			this.camera.lookAt(0, 0.5, 0);
		} else {
			// Desktop: normal position
			this.baseCameraZ = 8;
			this.camera.position.set(0, 1.5, this.baseCameraZ);
			this.camera.lookAt(0, 0.5, 0);
		}

		this.camera.updateProjectionMatrix();
		this.renderer.setSize(width, height);

		// Resize ASCII renderer
		if (this.asciiRenderer) {
			this.asciiRenderer.resize();
		}
	}

	/**
	 * Load animal models based on configs
	 */
	async loadAnimals(configs: AnimalConfigWithBand[]): Promise<void> {
		// Clear existing animals
		this.disposeAnimals();

		const loadPromises = configs.map(async (config, index) => {
			try {
				const gltf = await this.loader.loadAsync(config.modelPath);
				const model = gltf.scene;

				// Apply transforms
				model.position.set(...config.position);
				model.rotation.y = config.rotation;
				model.scale.setScalar(config.scale);

				// Ensure all materials are visible - disable reflections, boost dark parts
				model.traverse((child) => {
					if (child instanceof THREE.Mesh && child.material) {
						const materials = Array.isArray(child.material) ? child.material : [child.material];
						for (const mat of materials) {
							if (mat instanceof THREE.MeshStandardMaterial) {
								// DISABLE reflections - reflective materials appear black without envmap
								mat.metalness = 0;
								mat.roughness = 1;
								// Boost dark materials with emissive so they're visible
								if (mat.color) {
									const r = mat.color.r, g = mat.color.g, b = mat.color.b;
									const luminance = r * 0.299 + g * 0.587 + b * 0.114;
									if (luminance < 0.3) {
										// Dark material - add emissive matching its color but brighter
										mat.emissive = new THREE.Color(
											Math.min(1, r + 0.4),
											Math.min(1, g + 0.4),
											Math.min(1, b + 0.4)
										);
									}
								}
							}
						}
					}
				});

				// Add to scene
				this.scene.add(model);

				// Create controller with frequency band and phase offset
				const phaseOffset = index / configs.length;
				const controller = new AnimalController(model, config.frequencyBand, phaseOffset);
				this.animals.push(controller);

				console.log(`Loaded ${config.name} (${config.frequencyBand})`);
			} catch (error) {
				console.error(`Failed to load ${config.modelPath}:`, error);
			}
		});

		await Promise.all(loadPromises);
	}

	/**
	 * Connect audio analyzer
	 */
	connectAudio(analyser: AnalyserNode, sampleRate: number): void {
		this.audioAnalyzer = new AudioAnalyzer(analyser, sampleRate);
	}

	/**
	 * Disconnect audio analyzer
	 */
	disconnectAudio(): void {
		if (this.audioAnalyzer) {
			this.audioAnalyzer.reset();
		}
		this.audioAnalyzer = null;
	}

	/**
	 * Start animation loop
	 */
	start(): void {
		if (this.isPlaying) return;
		this.isPlaying = true;
		this.clock.start();
		this.animate();
	}

	/**
	 * Stop animation loop
	 */
	stop(): void {
		this.isPlaying = false;
		if (this.animationFrameId !== null) {
			cancelAnimationFrame(this.animationFrameId);
			this.animationFrameId = null;
		}
	}

	private debugCounter = 0;

	/**
	 * Animation loop
	 */
	private animate(): void {
		if (!this.isPlaying) return;

		this.animationFrameId = requestAnimationFrame(() => this.animate());

		const delta = this.clock.getDelta();

		// Get audio data
		let bands: AudioBands = { bass: 0, mids: 0, highs: 0, overall: 0 };
		let beat: BeatState = { isBeat: false, beatCount: 0, timeSinceLastBeat: 1000, bpm: 120 };
		let bandBeats = { bass: false, mids: false, highs: false };

		if (this.audioAnalyzer) {
			bands = this.audioAnalyzer.getBands();
			beat = this.audioAnalyzer.getBeatState();
			bandBeats = this.audioAnalyzer.getBandBeats();

			// Debug log every 60 frames (~1 second)
			this.debugCounter++;
			if (this.debugCounter % 60 === 0) {
				console.log('Audio bands:', bands, 'BPM:', beat.bpm, 'Band beats:', bandBeats);
			}
		}

		// Update all animals with their band-specific beat
		for (const animal of this.animals) {
			const bandBeat = bandBeats[animal.getFrequencyBand()];
			animal.update(delta, bands, beat, bandBeat);
		}

		// Add subtle camera movement based on audio
		this.updateCamera(bands, beat);

		// Render - either ASCII or normal
		if (this.asciiRenderer && this.asciiEnabled) {
			this.asciiRenderer.render(this.scene, this.camera, bands);
		} else {
			this.renderer.render(this.scene, this.camera);
		}
	}

	/**
	 * Toggle ASCII rendering mode
	 */
	setAsciiEnabled(enabled: boolean): void {
		this.asciiEnabled = enabled;
		if (enabled && !this.asciiRenderer) {
			this.asciiRenderer = new AsciiRenderer(this.renderer, this.container);
			this.asciiRenderer.resize();
		}
	}

	// Base camera Z position (adjusted on resize for mobile)
	private baseCameraZ = 8;

	/**
	 * Subtle camera movement based on audio
	 */
	private updateCamera(bands: AudioBands, beat: BeatState): void {
		// Subtle zoom on bass
		const targetZ = this.baseCameraZ - bands.bass * 0.8;
		this.camera.position.z += (targetZ - this.camera.position.z) * 0.05;

		// Subtle sway on beat
		if (beat.isBeat) {
			this.camera.position.x += (Math.random() - 0.5) * 0.15;
		}
		// Return to center
		this.camera.position.x *= 0.95;
	}

	/**
	 * Dispose of animal resources
	 */
	private disposeAnimals(): void {
		for (const animal of this.animals) {
			const model = animal.getModel();
			this.scene.remove(model);
			animal.dispose();

			// Dispose geometries and materials
			model.traverse((child) => {
				if (child instanceof THREE.Mesh) {
					child.geometry?.dispose();
					if (Array.isArray(child.material)) {
						child.material.forEach((m) => m.dispose());
					} else {
						child.material?.dispose();
					}
				}
			});
		}
		this.animals = [];
	}

	/**
	 * Clean up all resources
	 */
	dispose(): void {
		this.stop();
		window.removeEventListener('resize', this.boundHandleResize);

		this.disposeAnimals();

		// Dispose ASCII renderer
		if (this.asciiRenderer) {
			this.asciiRenderer.dispose();
			this.asciiRenderer = null;
		}

		this.renderer.dispose();

		if (this.container.contains(this.renderer.domElement)) {
			this.container.removeChild(this.renderer.domElement);
		}
	}
}
