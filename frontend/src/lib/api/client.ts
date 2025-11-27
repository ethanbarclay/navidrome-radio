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

	async getCurrentUser() {
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

	// Get track details by IDs
	async getTracksByIds(ids: string[]): Promise<{
		tracks: Array<{ id: string; title: string; artist: string; album: string }>;
	}> {
		return request('/library/tracks', {
			method: 'POST',
			body: JSON.stringify({ ids })
		});
	}
};
