import type * as THREE from 'three';

// Audio frequency band data extracted from Web Audio API
export interface AudioBands {
	bass: number;    // 0-1, 20-258Hz (kick drums, bass)
	mids: number;    // 0-1, 258-4kHz (vocals, snare, guitars)
	highs: number;   // 0-1, 4k-14kHz (hi-hats, cymbals)
	overall: number; // 0-1, average amplitude
}

// Beat detection state
export interface BeatState {
	isBeat: boolean;           // True on beat frame
	beatCount: number;         // Cumulative beat count
	timeSinceLastBeat: number; // Milliseconds since last beat
	bpm: number;               // Estimated BPM (60-200 range)
}

// Configuration for individual animal
export interface AnimalConfig {
	modelPath: string;                    // Path to GLB file
	position: [number, number, number];   // World position [x, y, z]
	rotation: number;                     // Y-axis rotation in radians
	scale: number;                        // Uniform scale factor
}

// Animation mapping from audio bands to clip names
export interface AnimationMapping {
	idle: string[];     // Default animations when low activity
	bass: string[];     // Triggered on bass response
	mids: string[];     // Triggered on mids response
	highs: string[];    // Triggered on highs response
	beat: string[];     // Triggered on beat detection
}

// State for an active dancing animal
export interface DancingAnimalState {
	model: THREE.Group;
	mixer: THREE.AnimationMixer;
	actions: Map<string, THREE.AnimationAction>;
	currentAction: THREE.AnimationAction | null;
	animationMapping: AnimationMapping;
	lastActionChangeTime: number;
}

// Available animal models
export const AVAILABLE_ANIMALS = [
	'chicken_001',
	'deer_001',
	'dog_001',
	'horse_001',
	'kitty_001',
	'pinguin_001',
	'tiger_001'
] as const;

export type AnimalName = typeof AVAILABLE_ANIMALS[number];

// Frequency band assignment for each animal
export type FrequencyBand = 'bass' | 'mids' | 'highs';

// Extended config with frequency band
export interface AnimalConfigWithBand extends AnimalConfig {
	frequencyBand: FrequencyBand;
	name: string;
}

// Default 3-animal configuration:
// - Cat (left) → Bass (body groove, big movements)
// - Dog (center) → Mids (marching, arm waves)
// - Chicken (right) → Highs (head bob, quick movements)
export const DEFAULT_ANIMAL_CONFIGS: AnimalConfigWithBand[] = [
	{
		name: 'cat',
		modelPath: '/models/animals/kitty_001.glb',
		position: [-4.5, -0.9, 0],
		rotation: Math.PI,
		scale: 12.0,
		frequencyBand: 'bass'
	},
	{
		name: 'chihuahua',
		modelPath: '/models/animals/chihuahua_derp_dog.glb',
		position: [0, -1, 1.5],
		rotation: 0,
		scale: 5.5,
		frequencyBand: 'mids'
	},
	{
		name: 'chicken',
		modelPath: '/models/animals/chicken_001.glb',
		position: [4.5, -1.2, 0],
		rotation: Math.PI,
		scale: 7.0, // Chicken model is smaller, scale up
		frequencyBand: 'highs'
	}
];
