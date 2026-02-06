import type { AudioBands, BeatState } from './types';

/**
 * Enhanced audio analyzer with per-band beat detection
 */
export class AudioAnalyzer {
	private analyser: AnalyserNode;
	private dataArray: Uint8Array<ArrayBuffer>;
	private sampleRate: number;

	// Frequency boundaries
	private readonly BASS_LOW = 20;
	private readonly BASS_HIGH = 258;
	private readonly MIDS_HIGH = 4000;
	private readonly HIGHS_HIGH = 14000;

	// Per-band beat detection
	private bassHistory: number[] = [];
	private midsHistory: number[] = [];
	private highsHistory: number[] = [];

	private lastBassBeat = 0;
	private lastMidsBeat = 0;
	private lastHighsBeat = 0;

	private bassBeatCount = 0;
	private midsBeatCount = 0;
	private highsBeatCount = 0;

	// Overall beat detection
	private beatHistory: number[] = [];
	private lastBeatTime = 0;
	private beatCount = 0;

	// Smoothed values
	private smoothedBass = 0;
	private smoothedMids = 0;
	private smoothedHighs = 0;

	// Rhythm tracking
	private beatIntervals: number[] = [];
	private estimatedBPM = 120;

	constructor(analyser: AnalyserNode, sampleRate: number = 44100) {
		this.analyser = analyser;
		this.sampleRate = sampleRate;
		const buffer = new ArrayBuffer(analyser.frequencyBinCount);
		this.dataArray = new Uint8Array(buffer);
	}

	getBands(): AudioBands {
		this.analyser.getByteFrequencyData(this.dataArray);

		const binWidth = this.sampleRate / this.analyser.fftSize;

		const bassStart = Math.floor(this.BASS_LOW / binWidth);
		const bassEnd = Math.floor(this.BASS_HIGH / binWidth);
		const midsEnd = Math.floor(this.MIDS_HIGH / binWidth);
		const highsEnd = Math.min(Math.floor(this.HIGHS_HIGH / binWidth), this.dataArray.length - 1);

		// Raw values
		const rawBass = this.averageRange(bassStart, bassEnd);
		const rawMids = this.averageRange(bassEnd, midsEnd);
		const rawHighs = this.averageRange(midsEnd, highsEnd);

		// Smoothing with fast attack, slow release
		this.smoothedBass = rawBass > this.smoothedBass
			? this.smoothedBass * 0.5 + rawBass * 0.5
			: this.smoothedBass * 0.85 + rawBass * 0.15;

		this.smoothedMids = rawMids > this.smoothedMids
			? this.smoothedMids * 0.4 + rawMids * 0.6
			: this.smoothedMids * 0.8 + rawMids * 0.2;

		this.smoothedHighs = rawHighs > this.smoothedHighs
			? this.smoothedHighs * 0.3 + rawHighs * 0.7
			: this.smoothedHighs * 0.75 + rawHighs * 0.25;

		return {
			bass: this.smoothedBass,
			mids: this.smoothedMids,
			highs: this.smoothedHighs,
			overall: (this.smoothedBass + this.smoothedMids + this.smoothedHighs) / 3
		};
	}

	/**
	 * Detect beat in a specific frequency band
	 */
	detectBandBeat(band: 'bass' | 'mids' | 'highs'): boolean {
		const bands = this.getBands();
		const now = performance.now();

		let history: number[];
		let lastBeat: number;
		let minInterval: number;
		let threshold: number;
		let value: number;

		switch (band) {
			case 'bass':
				history = this.bassHistory;
				lastBeat = this.lastBassBeat;
				minInterval = 200; // ~300 BPM max for bass
				threshold = 0.15;
				value = bands.bass;
				break;
			case 'mids':
				history = this.midsHistory;
				lastBeat = this.lastMidsBeat;
				minInterval = 100; // Faster for mids (snare)
				threshold = 0.12;
				value = bands.mids;
				break;
			case 'highs':
				history = this.highsHistory;
				lastBeat = this.lastHighsBeat;
				minInterval = 50; // Very fast for hi-hats
				threshold = 0.10;
				value = bands.highs;
				break;
		}

		// Add to history
		history.push(value);
		if (history.length > 30) history.shift();

		// Calculate dynamic threshold
		const avg = history.reduce((a, b) => a + b, 0) / history.length;
		const variance = history.reduce((sum, v) => sum + Math.pow(v - avg, 2), 0) / history.length;
		const dynamicThreshold = avg + Math.max(threshold, 1.3 * Math.sqrt(variance));

		// Detect beat
		const isBeat = value > dynamicThreshold && (now - lastBeat) > minInterval;

		if (isBeat) {
			switch (band) {
				case 'bass':
					this.lastBassBeat = now;
					this.bassBeatCount++;
					break;
				case 'mids':
					this.lastMidsBeat = now;
					this.midsBeatCount++;
					break;
				case 'highs':
					this.lastHighsBeat = now;
					this.highsBeatCount++;
					break;
			}
		}

		return isBeat;
	}

	/**
	 * Get overall beat state (for backwards compatibility)
	 */
	getBeatState(): BeatState {
		const bands = this.getBands();
		const now = performance.now();

		// Use bass for main beat detection
		this.beatHistory.push(bands.bass);
		if (this.beatHistory.length > 43) this.beatHistory.shift();

		const avg = this.beatHistory.reduce((a, b) => a + b, 0) / this.beatHistory.length;
		const variance = this.beatHistory.reduce((sum, e) => sum + Math.pow(e - avg, 2), 0) / this.beatHistory.length;
		const threshold = avg + 1.4 * Math.sqrt(variance);

		const minInterval = 180;
		const isBeat = bands.bass > Math.max(threshold, 0.3) && (now - this.lastBeatTime) > minInterval;

		if (isBeat) {
			const interval = now - this.lastBeatTime;
			this.beatIntervals.push(interval);
			if (this.beatIntervals.length > 8) this.beatIntervals.shift();

			this.beatCount++;
			this.lastBeatTime = now;

			// Update BPM estimate
			if (this.beatIntervals.length >= 3) {
				const avgInterval = this.beatIntervals.reduce((a, b) => a + b, 0) / this.beatIntervals.length;
				this.estimatedBPM = Math.round(60000 / avgInterval);
				this.estimatedBPM = Math.max(60, Math.min(200, this.estimatedBPM));
			}
		}

		return {
			isBeat,
			beatCount: this.beatCount,
			timeSinceLastBeat: now - this.lastBeatTime,
			bpm: this.estimatedBPM
		};
	}

	/**
	 * Get per-band beat info
	 */
	getBandBeats(): { bass: boolean; mids: boolean; highs: boolean } {
		return {
			bass: this.detectBandBeat('bass'),
			mids: this.detectBandBeat('mids'),
			highs: this.detectBandBeat('highs')
		};
	}

	/**
	 * Get time since last beat for a specific band
	 */
	getTimeSinceBandBeat(band: 'bass' | 'mids' | 'highs'): number {
		const now = performance.now();
		switch (band) {
			case 'bass': return now - this.lastBassBeat;
			case 'mids': return now - this.lastMidsBeat;
			case 'highs': return now - this.lastHighsBeat;
		}
	}

	private averageRange(start: number, end: number): number {
		if (start >= end || start >= this.dataArray.length) return 0;
		const actualEnd = Math.min(end, this.dataArray.length - 1);
		let sum = 0;
		for (let i = start; i <= actualEnd; i++) {
			sum += this.dataArray[i];
		}
		return sum / ((actualEnd - start + 1) * 255);
	}

	reset(): void {
		this.bassHistory = [];
		this.midsHistory = [];
		this.highsHistory = [];
		this.beatHistory = [];
		this.beatIntervals = [];
		this.lastBeatTime = 0;
		this.lastBassBeat = 0;
		this.lastMidsBeat = 0;
		this.lastHighsBeat = 0;
		this.beatCount = 0;
		this.smoothedBass = 0;
		this.smoothedMids = 0;
		this.smoothedHighs = 0;
	}
}
