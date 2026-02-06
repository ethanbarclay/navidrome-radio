import type { AuthResponse, Station, NowPlaying } from '$lib/types';

const API_BASE = '/api/v1';

// Curation progress types
export interface CurationProgress {
	step: 'started' | 'checking_cache' | 'analyzing_library' | 'ai_analyzing_query' | 'searching_tracks' | 'ai_selecting_tracks' | 'validating' | 'completed' | 'error';
	message: string;
	query?: string;
	thinking?: string;
	filters_applied?: Record<string, unknown>;
	candidate_count?: number;
	tracks_validated?: number;
	tracks_rejected?: number;
	tracks_selected?: number;
	reasoning?: string;
}

// Embedding progress types
export interface EmbeddingProgress {
	type: 'started' | 'processing' | 'track_complete' | 'track_error' | 'completed' | 'error';
	message?: string;
	total_tracks?: number;
	current?: number;
	total?: number;
	completed?: number;  // Number of completed tracks (for processing type)
	success_count?: number;
	error_count?: number;
	in_progress?: string[];  // Track names currently being processed in parallel
	current_track?: string;  // Legacy single track (kept for backwards compatibility)
	track_id?: string;
	track_name?: string;
	processing_time_ms?: number;
	error?: string;
	total_time_secs?: number;
}

// Hybrid curation progress types (matches backend HybridCurationProgress enum)
export interface HybridCurationProgress {
	step: 'started' | 'checking_embeddings' | 'selecting_seeds' | 'seeds_selected' | 'generating_embeddings' | 'filling_gaps' | 'completed' | 'error';
	message: string;
	query?: string;
	coverage_percent?: number;
	count?: number;
	seeds?: string[];
	// generating_embeddings fields
	current?: number;
	total?: number;
	track_name?: string;
	// filling_gaps fields
	segment?: number;
	total_segments?: number;
	from_seed?: string;
	to_seed?: string;
	// completed fields
	total_tracks?: number;
	seed_count?: number;
	filled_count?: number;
	method?: string;
	track_ids?: string[];
}

// Two-phase curation types
export interface SeedTrack {
	id: string;
	title: string;
	artist: string;
	album: string;
}

export interface SelectSeedsResponse {
	seeds: SeedTrack[];
	query: string;
	genres: string[];
}

export interface FillGapsResponse {
	track_ids: string[];
	tracks: Array<{ id: string; title: string; artist: string }>;
	seed_count: number;
	filled_count: number;
}

// Embedding visualization types
export interface EmbeddingPoint {
	id: string;
	title: string;
	artist: string;
	album: string;
	genre: string | null;
	x: number;
	y: number;
}

export interface EmbeddingVisualizationResponse {
	points: EmbeddingPoint[];
	cache_rebuilt: boolean;
}

function getAuthToken(): string | null {
	if (typeof localStorage === 'undefined') return null;
	return localStorage.getItem('auth_token');
}

function getHeaders(): HeadersInit {
	const headers: HeadersInit = {
		'Content-Type': 'application/json'
	};

	const token = getAuthToken();
	if (token) {
		headers['Authorization'] = `Bearer ${token}`;
	}

	return headers;
}

async function request<T>(url: string, options: RequestInit = {}): Promise<T> {
	const response = await fetch(`${API_BASE}${url}`, {
		...options,
		headers: {
			...getHeaders(),
			...options.headers
		}
	});

	if (!response.ok) {
		const error = await response.json().catch(() => ({ error: 'Unknown error' }));
		throw new Error(error.error || `HTTP ${response.status}`);
	}

	return response.json();
}

export const api = {
	// Auth
	async login(username: string, password: string): Promise<AuthResponse> {
		const response = await request<AuthResponse>('/auth/login', {
			method: 'POST',
			body: JSON.stringify({ username, password })
		});
		localStorage.setItem('auth_token', response.token);
		return response;
	},

	async register(username: string, email: string, password: string): Promise<AuthResponse> {
		const response = await request<AuthResponse>('/auth/register', {
			method: 'POST',
			body: JSON.stringify({ username, email, password })
		});
		localStorage.setItem('auth_token', response.token);
		return response;
	},

	async logout(): Promise<void> {
		localStorage.removeItem('auth_token');
	},

	async getCurrentUser(): Promise<{ id: string; username: string; email: string; role: 'admin' | 'listener' }> {
		return request('/auth/me');
	},

	// Stations
	async getStations(): Promise<Station[]> {
		return request('/stations');
	},

	async getStation(id: string): Promise<Station> {
		return request(`/stations/${id}`);
	},

	async createStation(data: {
		path: string;
		name: string;
		description: string;
		genres: string[];
		mood_tags?: string[];
		config?: Partial<any>;
		track_ids?: string[];
	}): Promise<Station> {
		return request('/stations', {
			method: 'POST',
			body: JSON.stringify(data)
		});
	},

	async updateStation(
		id: string,
		data: {
			name?: string;
			description?: string;
			genres?: string[];
			mood_tags?: string[];
			config?: Partial<any>;
		}
	): Promise<Station> {
		return request(`/stations/${id}`, {
			method: 'PATCH',
			body: JSON.stringify(data)
		});
	},

	async deleteStation(id: string): Promise<void> {
		return request(`/stations/${id}`, {
			method: 'DELETE'
		});
	},

	async startStation(id: string): Promise<void> {
		return request(`/stations/${id}/start`, {
			method: 'POST'
		});
	},

	async stopStation(id: string): Promise<void> {
		return request(`/stations/${id}/stop`, {
			method: 'POST'
		});
	},

	async skipTrack(id: string): Promise<void> {
		return request(`/stations/${id}/skip`, {
			method: 'POST'
		});
	},

	async getNowPlaying(id: string): Promise<NowPlaying> {
		return request(`/stations/${id}/nowplaying`);
	},

	// Listener tracking
	async listenerHeartbeat(stationId: string, sessionId: string): Promise<{ listeners: number }> {
		return request(`/stations/${stationId}/listener/heartbeat`, {
			method: 'POST',
			body: JSON.stringify({ session_id: sessionId })
		});
	},

	async listenerLeave(stationId: string, sessionId: string): Promise<void> {
		return request(`/stations/${stationId}/listener/leave`, {
			method: 'POST',
			body: JSON.stringify({ session_id: sessionId })
		});
	},

	async getListenerCounts(): Promise<{ counts: Record<string, number> }> {
		return request('/stations/listeners');
	},

	// AI Capabilities
	async getAiCapabilities(): Promise<{ available: boolean; features: string[] }> {
		return request('/ai/capabilities');
	},

	async analyzeDescription(description: string): Promise<{
		genres: string[];
		tracks_found: number;
		sample_tracks: string[];
	}> {
		return request('/ai/analyze-description', {
			method: 'POST',
			body: JSON.stringify({ description })
		});
	},

	// Library Curation (new AI-powered endpoint)
	async curateLibrary(query: string, limit?: number): Promise<{
		track_ids: string[];
		tracks: Array<{ id: string; title: string; artist: string }>;
		query: string;
	}> {
		return request('/library/curate', {
			method: 'POST',
			body: JSON.stringify({ query, limit })
		});
	},

	// AI Curation with SSE progress streaming
	curateWithProgress(
		query: string,
		limit: number,
		onProgress: (progress: CurationProgress) => void,
		onComplete: (trackIds: string[]) => void,
		onError: (error: string) => void
	): () => void {
		const token = getAuthToken();
		if (!token) {
			onError('Not authenticated');
			return () => {};
		}

		// Use fetch with streaming for POST SSE (EventSource only supports GET)
		const controller = new AbortController();

		fetch(`${API_BASE}/ai/curate`, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				'Authorization': `Bearer ${token}`
			},
			body: JSON.stringify({ query, limit }),
			signal: controller.signal
		})
			.then(async (response) => {
				if (!response.ok) {
					throw new Error(`HTTP ${response.status}`);
				}

				const reader = response.body?.getReader();
				if (!reader) {
					throw new Error('No response body');
				}

				const decoder = new TextDecoder();
				let buffer = '';

				while (true) {
					const { done, value } = await reader.read();
					if (done) break;

					buffer += decoder.decode(value, { stream: true });

					// Parse SSE messages (data: {...}\n\n)
					const lines = buffer.split('\n');
					buffer = lines.pop() || '';

					for (const line of lines) {
						if (line.startsWith('data: ')) {
							try {
								const data = JSON.parse(line.slice(6));
								onProgress(data);

								// Check for completion with track_ids in reasoning
								if (data.step === 'completed' && data.reasoning) {
									try {
										const result = JSON.parse(data.reasoning);
										if (result.track_ids) {
											onComplete(result.track_ids);
										}
									} catch {
										// reasoning wasn't JSON, that's fine
									}
								}

								// Check for error
								if (data.step === 'error') {
									onError(data.message);
								}
							} catch (e) {
								console.warn('Failed to parse SSE message:', line);
							}
						}
					}
				}
			})
			.catch((e) => {
				if (e.name !== 'AbortError') {
					onError(e.message);
				}
			});

		// Return cleanup function
		return () => controller.abort();
	},

	// Library Management
	async syncLibrary(): Promise<{ message: string; status: string }> {
		return request('/library/sync', {
			method: 'POST'
		});
	},

	async getLibraryStats(): Promise<{
		total_tracks: number;
		total_ai_analyzed: number;
		computed_at: string | null;
	}> {
		return request('/library/stats');
	},

	async getSyncStatus(): Promise<{
		sync_in_progress: boolean;
		last_sync_started: string | null;
		last_sync_completed: string | null;
		tracks_synced: number;
	}> {
		return request('/library/sync-status');
	},

	// Station Tracks
	async getStationTracks(stationId: string, limit?: number): Promise<{
		tracks: Array<{ id: string; title: string; artist: string; album: string; played_at?: string }>;
		total: number;
	}> {
		const params = limit ? `?limit=${limit}` : '';
		return request(`/stations/${stationId}/tracks${params}`);
	},

	// Create Navidrome playlist from station tracks
	async createNavidromePlaylist(stationId: string, name?: string): Promise<{
		playlist_id: string;
		name: string;
		track_count: number;
	}> {
		return request(`/stations/${stationId}/playlist`, {
			method: 'POST',
			body: JSON.stringify({ name })
		});
	},

	// Get track details by IDs
	async getTracksByIds(ids: string[]): Promise<{
		tracks: Array<{ id: string; title: string; artist: string; album: string }>;
	}> {
		return request('/library/tracks', {
			method: 'POST',
			body: JSON.stringify({ ids })
		});
	},

	// Audio Embedding APIs
	async getEmbeddingStatus(): Promise<{
		total_tracks: number;
		tracks_with_embeddings: number;
		coverage_percent: number;
		indexing_in_progress: boolean;
	}> {
		return request('/embeddings/status');
	},

	async startEmbeddingIndex(batchSize?: number, maxTracks?: number): Promise<{
		message: string;
		status: string;
	}> {
		return request('/embeddings/index', {
			method: 'POST',
			body: JSON.stringify({
				batch_size: batchSize,
				max_tracks: maxTracks
			})
		});
	},

	async pauseEmbeddings(): Promise<{ message: string; status: string }> {
		return request('/embeddings/pause', { method: 'POST' });
	},

	async resumeEmbeddings(): Promise<{ message: string; status: string }> {
		return request('/embeddings/resume', { method: 'POST' });
	},

	async stopEmbeddings(): Promise<{ message: string; status: string }> {
		return request('/embeddings/stop', { method: 'POST' });
	},

	// Hybrid AI Curation with SSE progress streaming
	hybridCurateWithProgress(
		query: string,
		limit: number,
		onProgress: (progress: HybridCurationProgress) => void,
		onComplete: (trackIds: string[]) => void,
		onError: (error: string) => void
	): () => void {
		const token = getAuthToken();
		if (!token) {
			onError('Not authenticated');
			return () => {};
		}

		// Build URL with query params for SSE (EventSource only supports GET)
		const url = new URL(`${API_BASE}/ai/hybrid-curate-stream`, window.location.origin);
		url.searchParams.set('token', token);
		url.searchParams.set('query', query);
		url.searchParams.set('limit', limit.toString());

		const eventSource = new EventSource(url.toString());

		eventSource.onmessage = (event) => {
			try {
				const data: HybridCurationProgress = JSON.parse(event.data);
				onProgress(data);

				// Check for completion with track_ids
				if (data.step === 'completed' && data.track_ids) {
					onComplete(data.track_ids);
					eventSource.close();
				}

				// Check for error
				if (data.step === 'error') {
					onError(data.message);
					eventSource.close();
				}
			} catch (e) {
				console.warn('Failed to parse SSE message:', event.data);
			}
		};

		eventSource.onerror = (e) => {
			console.error('SSE error:', e);
			onError('Connection error');
			eventSource.close();
		};

		// Return cleanup function
		return () => eventSource.close();
	},

	// Two-phase curation APIs
	async selectSeeds(query: string, seedCount?: number): Promise<SelectSeedsResponse> {
		return request('/ai/select-seeds', {
			method: 'POST',
			body: JSON.stringify({ query, seed_count: seedCount })
		});
	},

	async regenerateSeed(query: string, position: number, excludeIds: string[]): Promise<{ seed: SeedTrack; position: number }> {
		return request('/ai/regenerate-seed', {
			method: 'POST',
			body: JSON.stringify({ query, position, exclude_ids: excludeIds })
		});
	},

	async fillGaps(query: string, seedIds: string[], totalSize?: number): Promise<FillGapsResponse> {
		return request('/ai/fill-gaps', {
			method: 'POST',
			body: JSON.stringify({ query, seed_ids: seedIds, total_size: totalSize })
		});
	},

	// Embedding visualization
	async getEmbeddingsForVisualization(limit?: number): Promise<EmbeddingVisualizationResponse> {
		const params = limit ? `?limit=${limit}` : '';
		return request(`/embeddings/visualization${params}`);
	},

	// Settings
	async getSettings(): Promise<AppSettings> {
		return request('/settings');
	},

	async updateSettings(settings: Partial<AppSettings>): Promise<AppSettings> {
		return request('/settings', {
			method: 'PUT',
			body: JSON.stringify(settings)
		});
	}
};

// Settings types
export interface AppSettings {
	site_title: string;
}
