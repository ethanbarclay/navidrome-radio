import * as THREE from 'three';
import type { AudioBands, BeatState, FrequencyBand } from './types';

/**
 * Controls dynamic full-body animation for dancing animals.
 * Includes body rotation, leaning, turning, and synced beat responses.
 */
export class AnimalController {
	private model: THREE.Group;
	private bones: Map<string, THREE.Bone> = new Map();
	private initialRotations: Map<string, THREE.Euler> = new Map();
	private frequencyBand: FrequencyBand;
	private phaseOffset: number;

	// Base transforms
	private basePosition: THREE.Vector3;
	private baseRotation: THREE.Euler;
	private baseScale: THREE.Vector3;

	// Audio state
	private currentIntensity = 0;
	private peakIntensity = 0;

	// Per-band beat state
	private bandBeatDecay = 0;
	private lastBandBeatTime = 0;
	private bandBeatCount = 0;

	// Full body movement state
	private bodyRotationY = 0;      // Turning left/right
	private bodyLean = 0;           // Leaning forward/back
	private bodyTilt = 0;           // Tilting side to side
	private bodySway = { x: 0, z: 0 }; // Position sway

	// Animation phase
	private animPhase = 0;

	constructor(model: THREE.Group, frequencyBand: FrequencyBand, phaseOffset: number = 0) {
		this.model = model;
		this.frequencyBand = frequencyBand;
		this.phaseOffset = phaseOffset * Math.PI * 2;

		this.basePosition = model.position.clone();
		this.baseRotation = model.rotation.clone();
		this.baseScale = model.scale.clone();

		this.findBones(model);

		for (const [name, bone] of this.bones) {
			this.initialRotations.set(name, bone.rotation.clone());
		}

		console.log(`${frequencyBand} animal - Found ${this.bones.size} bones`);
	}

	private findBones(object: THREE.Object3D): void {
		if (object instanceof THREE.Bone) {
			this.bones.set(object.name.toLowerCase(), object);
		}
		for (const child of object.children) {
			this.findBones(child);
		}
	}

	/**
	 * Update with band-specific beat detection
	 */
	update(delta: number, bands: AudioBands, beat: BeatState, bandBeat: boolean): void {
		const rawIntensity = bands[this.frequencyBand];

		// Fast attack, slow release
		if (rawIntensity > this.currentIntensity) {
			this.currentIntensity += (rawIntensity - this.currentIntensity) * 0.6;
		} else {
			this.currentIntensity += (rawIntensity - this.currentIntensity) * 0.12;
		}

		// Track peak
		if (this.currentIntensity > this.peakIntensity) {
			this.peakIntensity = this.currentIntensity;
		} else {
			this.peakIntensity *= 0.998;
		}

		// Handle band-specific beat
		if (bandBeat) {
			this.bandBeatDecay = 1.0;
			this.bandBeatCount++;
			this.lastBandBeatTime = performance.now();
		} else {
			this.bandBeatDecay *= 0.9;
		}

		// Phase advances with intensity
		const phaseSpeed = 1 + this.currentIntensity * 5;
		this.animPhase += delta * phaseSpeed;

		// Apply full-body dynamics
		this.updateFullBodyMovement(delta);

		// Apply skeletal animation
		switch (this.frequencyBand) {
			case 'bass':
				this.animateBassAnimal();
				break;
			case 'mids':
				this.animateMidsAnimal();
				break;
			case 'highs':
				this.animateHighsAnimal();
				break;
		}

		// Apply transforms
		this.applyBodyTransform();
	}

	/**
	 * Full body movement - subtle swaying, grounded
	 */
	private updateFullBodyMovement(delta: number): void {
		const intensity = this.currentIntensity;
		const beatPulse = this.bandBeatDecay;
		const phase = this.animPhase + this.phaseOffset;

		// Much more subtle body movement - keep animals grounded
		switch (this.frequencyBand) {
			case 'bass':
				// Subtle groove - slight turns only
				this.bodyRotationY += (Math.sin(phase * 0.3) * intensity * 0.12 - this.bodyRotationY) * 0.06;
				this.bodyTilt += (Math.sin(phase * 0.4) * intensity * 0.08 - this.bodyTilt) * 0.08;
				// Minimal position sway
				this.bodySway.x += (Math.sin(phase * 0.5) * intensity * 0.08 - this.bodySway.x) * 0.05;
				break;

			case 'mids':
				// Slightly more active but still subtle
				this.bodyRotationY += (Math.sin(phase * 0.6) * intensity * 0.15 - this.bodyRotationY) * 0.08;
				this.bodyTilt += (Math.sin(phase * 0.8) * intensity * 0.1 - this.bodyTilt) * 0.1;
				this.bodySway.x += (Math.sin(phase * 0.7) * intensity * 0.06 - this.bodySway.x) * 0.06;
				break;

			case 'highs':
				// Quick but small movements
				this.bodyRotationY += (Math.sin(phase * 1.2) * intensity * 0.1 - this.bodyRotationY) * 0.12;
				this.bodyTilt += (Math.sin(phase * 1.5) * intensity * 0.08 - this.bodyTilt) * 0.12;
				break;
		}

		// Remove lean entirely - it looks unnatural
		this.bodyLean *= 0.9;
		this.bodySway.z *= 0.9;
	}

	/**
	 * Animate ears - reactive to audio
	 */
	private animateEars(intensity: number, phase: number): void {
		const ears = ['ear.r', 'ear.l', 'ear_01.r', 'ear_01.l'];
		for (const earName of ears) {
			const bone = this.bones.get(earName);
			const initial = this.initialRotations.get(earName);
			if (bone && initial) {
				const isRight = earName.includes('.r');
				const sign = isRight ? 1 : -1;
				// Ears perk up on beats, twitch with highs
				bone.rotation.z = initial.z + Math.sin(phase * 1.5) * intensity * 0.3 * sign;
				bone.rotation.x = initial.x + Math.sin(phase * 2) * intensity * 0.2;
			}
		}
	}

	/**
	 * Animate shoulders/upper arms
	 */
	private animateShoulders(intensity: number, phase: number): void {
		const shoulders = [
			['shoulder.r', 'upper_arm.r'],
			['shoulder.l', 'upper_arm.l'],
			['front_thigh.r', 'front_shin.r'],
			['front_thigh.l', 'front_shin.l']
		];

		for (let i = 0; i < shoulders.length; i++) {
			const [upper, lower] = shoulders[i];
			const isRight = upper.includes('.r');
			const isFront = upper.includes('front');
			const sign = isRight ? 1 : -1;

			const upperBone = this.bones.get(upper);
			const upperInit = this.initialRotations.get(upper);
			const lowerBone = this.bones.get(lower);
			const lowerInit = this.initialRotations.get(lower);

			if (upperBone && upperInit) {
				// Subtle shoulder roll
				const armPhase = phase + (isRight ? 0 : Math.PI * 0.5);
				if (isFront) {
					upperBone.rotation.x = upperInit.x + Math.sin(armPhase) * intensity * 0.25;
				} else {
					upperBone.rotation.z = upperInit.z + Math.sin(armPhase * 0.8) * intensity * 0.15 * sign;
				}
			}

			if (lowerBone && lowerInit) {
				// Lower arm/leg follows
				const armPhase = phase + (isRight ? 0 : Math.PI * 0.5);
				lowerBone.rotation.x = lowerInit.x + Math.sin(armPhase * 1.2) * intensity * 0.15;
			}
		}
	}

	/**
	 * Animate jaw/mouth on beats
	 */
	private animateJaw(intensity: number, beatPulse: number): void {
		const jaw = this.bones.get('jaw') || this.bones.get('mouth');
		const initial = jaw ? this.initialRotations.get(jaw.name.toLowerCase()) : null;
		if (jaw && initial) {
			// Open mouth slightly on strong beats
			const openAmount = beatPulse * intensity * 0.3;
			jaw.rotation.x = initial.x + openAmount;
		}
	}

	/**
	 * Apply body transform to model
	 */
	private applyBodyTransform(): void {
		const intensity = this.currentIntensity;
		const beatPulse = this.bandBeatDecay;

		// Position: base + sway + subtle beat bounce
		let bounceY = 0;
		if (beatPulse > 0.1) {
			switch (this.frequencyBand) {
				case 'bass':
					bounceY = Math.sin(beatPulse * Math.PI) * 0.15 * intensity;
					break;
				case 'mids':
					bounceY = Math.sin(beatPulse * Math.PI) * 0.1 * intensity;
					break;
				case 'highs':
					bounceY = Math.sin(beatPulse * Math.PI) * 0.08 * intensity;
					break;
			}
		}

		this.model.position.x = this.basePosition.x + this.bodySway.x;
		this.model.position.y = this.basePosition.y + bounceY;
		this.model.position.z = this.basePosition.z + this.bodySway.z;

		// Rotation: base + body rotation + tilt + lean
		this.model.rotation.y = this.baseRotation.y + this.bodyRotationY;
		this.model.rotation.x = this.baseRotation.x + this.bodyLean;
		this.model.rotation.z = this.baseRotation.z + this.bodyTilt;

		// Subtle squash and stretch on beat (very subtle)
		let squash = 1;
		if (beatPulse > 0.2) {
			squash = 1 + beatPulse * 0.06 * intensity;
		}
		this.model.scale.y = this.baseScale.y * squash;
		this.model.scale.x = this.baseScale.x / Math.sqrt(squash);
		this.model.scale.z = this.baseScale.z / Math.sqrt(squash);
	}

	/**
	 * BASS animal - Heavy groovy movements
	 */
	private animateBassAnimal(): void {
		const intensity = this.currentIntensity;
		const phase = this.animPhase + this.phaseOffset;
		const beatPulse = this.bandBeatDecay;

		if (intensity < 0.03) {
			this.resetBones();
			return;
		}

		// Spine - smooth wave motion
		const spines = ['spine', 'spine.001', 'spine.002', 'spine.003', 'spine.004',
			'spine.005', 'spine.006', 'spine.007', 'spine.008', 'spine.009'];

		for (let i = 0; i < spines.length; i++) {
			const bone = this.bones.get(spines[i]);
			const initial = this.initialRotations.get(spines[i]);
			if (bone && initial) {
				const wave = Math.sin(phase * 0.5 + i * 0.2) * intensity * 0.2;
				bone.rotation.z = initial.z + wave;
				bone.rotation.x = initial.x + Math.sin(phase * 0.3 + i * 0.1) * intensity * 0.12;
			}
		}

		// Head - slow heavy nod
		this.animateHead(intensity * 0.6, phase * 0.6);

		// Ears react to bass hits
		this.animateEars(intensity * 0.8, phase * 0.8);

		// Shoulders/arms groove
		this.animateShoulders(intensity * 0.5, phase * 0.5);

		// Jaw opens on big beats
		this.animateJaw(intensity, beatPulse);

		// Legs - stomping
		this.animateLegs(intensity, phase, 0.5);

		// Tail
		this.animateTail(intensity, phase * 0.8);
	}

	/**
	 * MIDS animal - Active dancing
	 */
	private animateMidsAnimal(): void {
		const intensity = this.currentIntensity;
		const phase = this.animPhase + this.phaseOffset;
		const beatPulse = this.bandBeatDecay;

		if (intensity < 0.03) {
			this.resetBones();
			return;
		}

		// Spine - bouncy movement
		const spines = ['spine', 'spine.001', 'spine.002', 'spine.003', 'spine.004',
			'spine.005', 'spine.006', 'spine.007', 'spine.008', 'spine.009'];

		for (let i = 0; i < spines.length; i++) {
			const bone = this.bones.get(spines[i]);
			const initial = this.initialRotations.get(spines[i]);
			if (bone && initial) {
				bone.rotation.z = initial.z + Math.sin(phase * 0.8 + i * 0.15) * intensity * 0.2;
				bone.rotation.x = initial.x + Math.sin(phase * 0.6) * intensity * 0.12;
			}
		}

		// Head - active bobbing
		this.animateHead(intensity * 0.7, phase);

		// Ears perk up on snare
		this.animateEars(intensity, phase * 1.2);

		// Arms/shoulders more active
		this.animateShoulders(intensity * 0.7, phase);

		// Jaw on beats
		this.animateJaw(intensity * 0.8, beatPulse);

		// Legs - prancing
		this.animateLegs(intensity, phase, 0.6);

		// Tail - wagging
		this.animateTail(intensity, phase * 1.2);
	}

	/**
	 * HIGHS animal (chicken) - Quick head movements, wing flaps
	 */
	private animateHighsAnimal(): void {
		const intensity = this.currentIntensity;
		const phase = this.animPhase + this.phaseOffset;

		if (intensity < 0.03) {
			this.resetBones();
			return;
		}

		// Spine - subtle quick movement
		const spines = ['spine', 'spine.001', 'spine.002', 'spine.003', 'spine.004'];

		for (let i = 0; i < spines.length; i++) {
			const bone = this.bones.get(spines[i]);
			const initial = this.initialRotations.get(spines[i]);
			if (bone && initial) {
				bone.rotation.z = initial.z + Math.sin(phase * 1.5 + i * 0.2) * intensity * 0.15;
				bone.rotation.x = initial.x + Math.sin(phase) * intensity * 0.1;
			}
		}

		// Head - chicken pecking motion (more pronounced)
		this.animateHead(intensity, phase * 1.5);

		// Ears (comb for chicken)
		this.animateEars(intensity * 0.5, phase * 2);

		// Wings flap on hi-hats
		this.animateWings(intensity, phase * 1.5);

		// Legs - quick steps
		this.animateLegs(intensity, phase * 1.2, 0.4);

		// Tail feathers
		this.animateTail(intensity * 0.8, phase * 1.5);
	}

	private animateHead(intensity: number, phase: number): void {
		const head = this.bones.get('head') || this.bones.get('jaw');
		const initial = head ? this.initialRotations.get(head.name.toLowerCase()) : null;
		if (head && initial) {
			head.rotation.x = initial.x + Math.sin(phase) * intensity * 0.5;
			head.rotation.z = initial.z + Math.sin(phase * 0.7) * intensity * 0.3;
			head.rotation.y = initial.y + Math.sin(phase * 0.4) * intensity * 0.25;
		}
	}

	private animateLegs(intensity: number, phase: number, mult: number): void {
		const pairs = [
			['thigh.r', 'thigh.l'], ['front_thigh.r', 'front_thigh.l'],
			['shin.r', 'shin.l'], ['front_shin.r', 'front_shin.l']
		];

		for (let p = 0; p < pairs.length; p++) {
			const jMult = p < 2 ? 1.0 : 0.5;
			for (let s = 0; s < 2; s++) {
				const bone = this.bones.get(pairs[p][s]);
				const initial = this.initialRotations.get(pairs[p][s]);
				if (bone && initial) {
					const legPhase = phase + (s === 0 ? 0 : Math.PI);
					bone.rotation.x = initial.x + Math.sin(legPhase) * intensity * mult * jMult;
				}
			}
		}
	}

	private animateTail(intensity: number, phase: number): void {
		const tails = ['tail', 'tail.001', 'tail.002', 'tail.003', 'tail.004'];
		for (let i = 0; i < tails.length; i++) {
			const bone = this.bones.get(tails[i]);
			const initial = this.initialRotations.get(tails[i]);
			if (bone && initial) {
				const seg = 0.25 + i * 0.12;
				bone.rotation.y = initial.y + Math.sin(phase + i * 0.4) * intensity * seg;
				bone.rotation.z = initial.z + Math.cos(phase * 0.7 + i * 0.3) * intensity * seg * 0.5;
			}
		}
	}

	private animateWings(intensity: number, phase: number): void {
		for (const side of ['shoulder.r', 'shoulder.l']) {
			const bone = this.bones.get(side);
			const initial = this.initialRotations.get(side);
			if (bone && initial) {
				const sign = side.includes('.r') ? 1 : -1;
				bone.rotation.z = initial.z + Math.sin(phase * 2) * intensity * 0.5 * sign;
			}
		}
	}

	private resetBones(): void {
		for (const [name, bone] of this.bones) {
			const initial = this.initialRotations.get(name);
			if (initial) {
				bone.rotation.x += (initial.x - bone.rotation.x) * 0.08;
				bone.rotation.y += (initial.y - bone.rotation.y) * 0.08;
				bone.rotation.z += (initial.z - bone.rotation.z) * 0.08;
			}
		}
		// Reset body movement
		this.bodyRotationY *= 0.95;
		this.bodyLean *= 0.95;
		this.bodyTilt *= 0.95;
		this.bodySway.x *= 0.95;
		this.bodySway.z *= 0.95;
	}

	getModel(): THREE.Group {
		return this.model;
	}

	getFrequencyBand(): FrequencyBand {
		return this.frequencyBand;
	}

	dispose(): void {
		this.bones.clear();
		this.initialRotations.clear();
	}
}
