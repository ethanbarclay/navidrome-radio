<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.svelte';
	import type { NowPlaying, Station } from '$lib/types';

	let stationPath = $derived($page.params.path);
	let station = $state<Station | null>(null);
	let nowPlaying = $state<NowPlaying | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let audioElement = $state<HTMLAudioElement | undefined>();
	// Check if user has a saved preference
	const hasSavedPreference =
		typeof localStorage !== 'undefined' && localStorage.getItem('radio_muted') !== null;
	let isMuted = $state(
		typeof localStorage !== 'undefined' ? localStorage.getItem('radio_muted') === 'true' : false
	);
	let pollInterval: number;
	let currentTrackId = $state<string | null>(null);
	let currentPosition = $state(0);
	let progressInterval: number;
	let autoplayBlocked = $state(false);
	let needsUserInteraction = $state(false);
	let mediaSession: MediaSession | null = null;

	onMount(async () => {
		// Initialize Media Session API for Android notification center
		if ('mediaSession' in navigator) {
			mediaSession = navigator.mediaSession;
			setupMediaSession();
		}
		try {
			// Find station by path
			const stations = await api.getStations();
			station = stations.find((s) => s.path === stationPath) || null;

			if (!station) {
				error = 'Station not found';
				loading = false;
				return;
			}

			if (!station.active) {
				error = 'This station is not currently broadcasting';
				loading = false;
				return;
			}

			// Get now playing
			await updateNowPlaying();

			// Poll for updates every 3 seconds
			pollInterval = setInterval(updateNowPlaying, 3000);

			// Update progress bar every 500ms
			progressInterval = setInterval(updateProgress, 500);

			// Set up audio element event listeners for Media Session API
			const setupAudioListeners = () => {
				if (!audioElement) {
					setTimeout(setupAudioListeners, 100);
					return;
				}

				// Update media session playback state on play/pause
				audioElement.addEventListener('play', () => {
					if (mediaSession) {
						mediaSession.playbackState = 'playing';
						}
				});

				audioElement.addEventListener('pause', () => {
					if (mediaSession) {
						mediaSession.playbackState = 'paused';
						}
				});

				};

			setupAudioListeners();

			loading = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load station';
			loading = false;
		}
	});

	onDestroy(() => {
		if (pollInterval) {
			clearInterval(pollInterval);
		}
		if (progressInterval) {
			clearInterval(progressInterval);
		}
	});

	async function updateNowPlaying() {
		if (!station) return;

		try {
			const np = await api.getNowPlaying(station.id);

			// If track changed, sync to correct position
			if (np.track.id !== currentTrackId) {
				currentTrackId = np.track.id;
				nowPlaying = np;

				// Update media session metadata for new track
				updateMediaSession();

				// Wait a tick for the DOM to update with new audio element
				await new Promise(resolve => setTimeout(resolve, 100));

				// Wait for audio element to be ready, then sync
				syncAudioPosition(np);
			} else {
				nowPlaying = np;
			}
		} catch (e) {
			console.error('Failed to update now playing:', e);
		}
	}

	function setupMediaSession() {
		if (!mediaSession) return;

		// Set up media controls for notification center
		mediaSession.setActionHandler('play', () => {
			if (audioElement) {
				audioElement.play();
			}
		});

		mediaSession.setActionHandler('pause', () => {
			if (audioElement) {
				audioElement.pause();
			}
		});

		// Disable seek controls - this is a live radio stream
		mediaSession.setActionHandler('seekbackward', null);
		mediaSession.setActionHandler('seekforward', null);
		mediaSession.setActionHandler('seekto', null);

		// Disable previous/next (could be added later for station history)
		mediaSession.setActionHandler('previoustrack', null);
		mediaSession.setActionHandler('nexttrack', null);
	}

	async function updateMediaSession() {
		if (!mediaSession || !nowPlaying) return;

		const coverUrl = nowPlaying.track.albumArt
			? (nowPlaying.track.albumArt.startsWith('http')
				? nowPlaying.track.albumArt
				: `${window.location.origin}${nowPlaying.track.albumArt}`)
			: `${window.location.origin}/api/v1/navidrome/cover/${nowPlaying.track.id}`;

		try {
			// Fetch the image and create a blob URL for better compatibility
			const response = await fetch(coverUrl);

			if (!response.ok) {
				throw new Error('Failed to fetch cover');
			}

			const blob = await response.blob();
			const blobUrl = URL.createObjectURL(blob);

			mediaSession.metadata = new MediaMetadata({
				title: nowPlaying.track.title,
				artist: nowPlaying.track.artist,
				album: nowPlaying.track.album || station?.name || 'Navidrome Radio',
				artwork: [
					{ src: blobUrl, sizes: '96x96', type: blob.type },
					{ src: blobUrl, sizes: '128x128', type: blob.type },
					{ src: blobUrl, sizes: '192x192', type: blob.type },
					{ src: blobUrl, sizes: '256x256', type: blob.type },
					{ src: blobUrl, sizes: '384x384', type: blob.type },
					{ src: blobUrl, sizes: '512x512', type: blob.type }
				]
			});
		} catch {
			// Fallback to direct URL if blob fetch fails
			mediaSession.metadata = new MediaMetadata({
				title: nowPlaying.track.title,
				artist: nowPlaying.track.artist,
				album: nowPlaying.track.album || station?.name || 'Navidrome Radio',
				artwork: [
					{ src: coverUrl, sizes: '512x512', type: 'image/jpeg' }
				]
			});
		}

		mediaSession.playbackState = audioElement?.paused ? 'paused' : 'playing';
	}

	function syncAudioPosition(np: NowPlaying) {
		if (!audioElement) {
			setTimeout(() => syncAudioPosition(np), 100);
			return;
		}

		const startedAt = new Date(np.started_at).getTime();
		const now = Date.now();
		const elapsedSeconds = (now - startedAt) / 1000;

		const setPosition = () => {
			if (!audioElement) return;

			if (elapsedSeconds >= 0 && elapsedSeconds < np.track.duration) {
				try {
					audioElement.currentTime = elapsedSeconds;
					audioElement.muted = isMuted;
					audioElement.volume = 1.0;

					audioElement.play().then(() => {
						autoplayBlocked = false;
					}).catch(() => {
						// Browser blocked autoplay, must start muted
						if (audioElement) {
							const userWantedUnmuted = !isMuted;
							audioElement.muted = true;
							isMuted = true;

							audioElement.play().then(() => {
								if (userWantedUnmuted) {
									needsUserInteraction = true;
								}
							});
						}
					});
				} catch (err) {
					console.error('Error setting audio position:', err);
				}
			}
		};

		if (audioElement.readyState >= 2) {
			setPosition();
		} else {
			const onLoadedData = () => {
				setPosition();
				cleanup();
			};

			const onCanPlay = () => {
				setPosition();
				cleanup();
			};

			const cleanup = () => {
				audioElement?.removeEventListener('loadeddata', onLoadedData);
				audioElement?.removeEventListener('canplay', onCanPlay);
			};

			audioElement.addEventListener('loadeddata', onLoadedData, { once: true });
			audioElement.addEventListener('canplay', onCanPlay, { once: true });

			setTimeout(() => {
				if (audioElement && audioElement.readyState < 2) {
					setPosition();
					cleanup();
				}
			}, 2000);
		}
	}

	function updateProgress() {
		if (!nowPlaying) return;

		const startedAt = new Date(nowPlaying.started_at).getTime();
		const now = Date.now();
		const elapsed = (now - startedAt) / 1000;

		currentPosition = Math.min(elapsed, nowPlaying.track.duration);
	}

	function formatTime(seconds: number): string {
		const mins = Math.floor(seconds / 60);
		const secs = Math.floor(seconds % 60);
		return `${mins}:${secs.toString().padStart(2, '0')}`;
	}

	async function handleSkip() {
		if (!station || !authStore.isAdmin) return;

		try {
			await api.skipTrack(station.id);
			currentTrackId = null;
			await updateNowPlaying();
		} catch (e) {
			console.error('Failed to skip track:', e);
			alert('Failed to skip track: ' + (e instanceof Error ? e.message : 'Unknown error'));
		}
	}

	function toggleMute() {
		if (!audioElement) return;

		isMuted = !isMuted;
		audioElement.muted = isMuted;
		autoplayBlocked = false;
		needsUserInteraction = false;

		localStorage.setItem('radio_muted', isMuted.toString());

		if (!isMuted && audioElement.paused) {
			audioElement.play();
		}
	}

	function resumePlayback() {
		if (!audioElement) return;

		needsUserInteraction = false;
		isMuted = false;
		audioElement.muted = false;

		if (audioElement.paused) {
			audioElement.play();
		}
	}
</script>

{#if loading}
	<div class="min-h-screen flex items-center justify-center">
		<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
	</div>
{:else if error}
	<div class="min-h-screen flex items-center justify-center">
		<div class="text-center">
			<p class="text-red-500 text-xl mb-4">{error}</p>
			<a href="/" class="text-blue-400 hover:text-blue-300">Back to stations</a>
		</div>
	</div>
{:else if station && nowPlaying}
	<div class="min-h-screen flex flex-col bg-gray-900 text-white p-4 md:p-8">
		<audio
			bind:this={audioElement}
			src="/api/v1/navidrome/stream/{nowPlaying.track.id}"
		></audio>

		<!-- Station name at top -->
		<div class="text-center mb-8">
			<a href="/" class="text-blue-400 hover:text-blue-300 text-sm mb-2 inline-block">
				← Back to stations
			</a>
			<h1 class="text-2xl md:text-3xl font-bold">{station.name}</h1>
			<p class="text-gray-400 mt-2">{station.description}</p>
		</div>

		<!-- Now playing - centered -->
		<div class="flex-1 flex flex-col items-center justify-center space-y-4 md:space-y-6 max-w-2xl mx-auto w-full">
			<!-- Resume playback overlay -->
			{#if needsUserInteraction}
				<div class="fixed inset-0 bg-black bg-opacity-75 flex items-center justify-center z-50 backdrop-blur-sm">
					<div class="text-center space-y-6 p-8">
						<div class="text-white">
							<svg class="w-24 h-24 mx-auto mb-4 animate-pulse" fill="currentColor" viewBox="0 0 20 20">
								<path
									fill-rule="evenodd"
									d="M10 18a8 8 0 100-16 8 8 0 000 16zM9.555 7.168A1 1 0 008 8v4a1 1 0 001.555.832l3-2a1 1 0 000-1.664l-3-2z"
									clip-rule="evenodd"
								/>
							</svg>
							<h2 class="text-2xl md:text-3xl font-bold mb-2">Welcome Back!</h2>
							<p class="text-gray-300 mb-6">Click below to resume listening</p>
						</div>
						<button
							onclick={resumePlayback}
							class="px-12 py-5 bg-blue-600 hover:bg-blue-700 text-white text-xl font-bold rounded-full transition-all transform hover:scale-105 active:scale-95 shadow-2xl"
						>
							Resume Playback
						</button>
					</div>
				</div>
			{/if}

			<!-- Album art -->
			{#if nowPlaying.track.albumArt}
				<img
					src={nowPlaying.track.albumArt}
					alt={nowPlaying.track.album}
					class="w-64 h-64 md:w-80 md:h-80 lg:w-96 lg:h-96 rounded-lg shadow-2xl object-cover"
				/>
			{:else}
				<div
					class="w-64 h-64 md:w-80 md:h-80 lg:w-96 lg:h-96 rounded-lg shadow-2xl bg-gradient-to-br from-blue-600 to-purple-600 flex items-center justify-center"
				>
					<svg class="w-32 h-32 text-white" fill="currentColor" viewBox="0 0 20 20">
						<path
							d="M18 3a1 1 0 00-1.196-.98l-10 2A1 1 0 006 5v9.114A4.369 4.369 0 005 14c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V7.82l8-1.6v5.894A4.37 4.37 0 0015 12c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V3z"
						/>
					</svg>
				</div>
			{/if}

			<!-- Track info -->
			<div class="text-center max-w-lg px-4 w-full space-y-4">
				<div>
					<h2 class="text-2xl md:text-3xl lg:text-4xl font-bold mb-2 truncate">
						{nowPlaying.track.title}
					</h2>
					<p class="text-lg md:text-xl text-gray-300 mb-1 truncate">
						{nowPlaying.track.artist}
					</p>
					<p class="text-sm md:text-base text-gray-400 truncate">
						{nowPlaying.track.album}
					</p>
				</div>

				<!-- Progress bar -->
				<div class="w-full px-2">
					<div class="flex items-center justify-between text-xs md:text-sm text-gray-400 mb-2">
						<span>{formatTime(currentPosition)}</span>
						<span>{formatTime(nowPlaying.track.duration)}</span>
					</div>
					<div class="w-full bg-gray-700 rounded-full h-1.5 md:h-2 overflow-hidden">
						<div
							class="bg-blue-500 h-full transition-all duration-200 ease-linear"
							style="width: {(currentPosition / nowPlaying.track.duration) * 100}%"
						></div>
					</div>
				</div>
			</div>

			<!-- Autoplay blocked notification -->
			{#if autoplayBlocked}
				<div class="bg-orange-600 text-white px-6 py-3 rounded-lg text-sm md:text-base font-medium animate-pulse">
					Click the speaker button below to hear audio
				</div>
			{/if}

			<!-- Mute/Unmute button -->
			<button
				onclick={toggleMute}
				class="w-16 h-16 md:w-20 md:h-20 bg-blue-600 hover:bg-blue-700 rounded-full flex items-center justify-center transition-colors active:scale-95 {autoplayBlocked ? 'ring-4 ring-orange-500 ring-offset-2 ring-offset-gray-900' : ''}"
				title={isMuted ? 'Unmute' : 'Mute'}
			>
				{#if isMuted}
					<!-- Muted icon -->
					<svg class="w-8 h-8 md:w-10 md:h-10" fill="currentColor" viewBox="0 0 20 20">
						<path
							fill-rule="evenodd"
							d="M9.383 3.076A1 1 0 0110 4v12a1 1 0 01-1.707.707L4.586 13H2a1 1 0 01-1-1V8a1 1 0 011-1h2.586l3.707-3.707a1 1 0 011.09-.217zM12.293 7.293a1 1 0 011.414 0L15 8.586l1.293-1.293a1 1 0 111.414 1.414L16.414 10l1.293 1.293a1 1 0 01-1.414 1.414L15 11.414l-1.293 1.293a1 1 0 01-1.414-1.414L13.586 10l-1.293-1.293a1 1 0 010-1.414z"
							clip-rule="evenodd"
						/>
					</svg>
				{:else}
					<!-- Unmuted icon -->
					<svg class="w-8 h-8 md:w-10 md:h-10" fill="currentColor" viewBox="0 0 20 20">
						<path
							fill-rule="evenodd"
							d="M9.383 3.076A1 1 0 0110 4v12a1 1 0 01-1.707.707L4.586 13H2a1 1 0 01-1-1V8a1 1 0 011-1h2.586l3.707-3.707a1 1 0 011.09-.217zM14.657 2.929a1 1 0 011.414 0A9.972 9.972 0 0119 10a9.972 9.972 0 01-2.929 7.071 1 1 0 01-1.414-1.414A7.971 7.971 0 0017 10c0-2.21-.894-4.208-2.343-5.657a1 1 0 010-1.414zm-2.829 2.828a1 1 0 011.415 0A5.983 5.983 0 0115 10a5.984 5.984 0 01-1.757 4.243 1 1 0 01-1.415-1.415A3.984 3.984 0 0013 10a3.983 3.983 0 00-1.172-2.828 1 1 0 010-1.415z"
							clip-rule="evenodd"
						/>
					</svg>
				{/if}
			</button>
		</div>

		<!-- Controls at bottom -->
		<div class="mt-8 space-y-4">
			<div class="flex items-center justify-center gap-2 text-sm md:text-base text-gray-400">
				<svg class="w-4 h-4 md:w-5 md:h-5" fill="currentColor" viewBox="0 0 20 20">
					<path
						d="M9 6a3 3 0 11-6 0 3 3 0 016 0zM17 6a3 3 0 11-6 0 3 3 0 016 0zM12.93 17c.046-.327.07-.66.07-1a6.97 6.97 0 00-1.5-4.33A5 5 0 0119 16v1h-6.07zM6 11a5 5 0 015 5v1H1v-1a5 5 0 015-5z"
					/>
				</svg>
				<span>{nowPlaying.listeners} {nowPlaying.listeners === 1 ? 'listener' : 'listeners'}</span
				>
			</div>

			{#if authStore.isAdmin}
				<button
					onclick={handleSkip}
					class="w-full md:w-auto mx-auto block px-8 py-4 md:py-3 bg-orange-600 hover:bg-orange-700 rounded-lg font-semibold text-base md:text-lg transition-colors active:scale-95"
				>
					Skip Track
				</button>
			{/if}
		</div>
	</div>
{/if}

<style>
	.truncate {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
