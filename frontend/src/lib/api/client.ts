import type { AuthResponse, Station, NowPlaying } from '$lib/types';

const API_BASE = '/api/v1';

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
	}
};
