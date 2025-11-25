export interface User {
	id: string;
	username: string;
	email: string;
	role: 'admin' | 'listener';
}

export interface AuthResponse {
	token: string;
	user: User;
}

export interface Station {
	id: string;
	path: string;
	name: string;
	description: string;
	genres: string[];
	mood_tags: string[];
	created_by: string;
	created_at: string;
	updated_at: string;
	active: boolean;
	config: StationConfig;
}

export interface StationConfig {
	bitrate: number;
	sample_rate: number;
	crossfade_ms: number;
	track_selection_mode: 'ai_contextual' | 'ai_embeddings' | 'random' | 'hybrid';
	min_track_duration: number;
	max_track_duration: number;
	explicit_content: boolean;
}

export interface Track {
	id: string;
	title: string;
	artist: string;
	album: string;
	duration: number;
	albumArt?: string;
}

export interface NowPlaying {
	track: Track;
	started_at: string;
	listeners: number;
}
