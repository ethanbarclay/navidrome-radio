import type { User } from '$lib/types';
import { api } from '$lib/api/client';

interface AuthState {
	user: User | null;
	loading: boolean;
}

const state = $state<AuthState>({
	user: null,
	loading: true
});

export const authStore = {
	get user() {
		return state.user;
	},
	get loading() {
		return state.loading;
	},
	get isAuthenticated() {
		return state.user !== null;
	},
	get isAdmin() {
		return state.user?.role === 'admin';
	},

	async login(username: string, password: string) {
		const response = await api.login(username, password);
		state.user = response.user;
		return response;
	},

	async register(username: string, email: string, password: string) {
		const response = await api.register(username, email, password);
		state.user = response.user;
		return response;
	},

	async logout() {
		await api.logout();
		state.user = null;
	},

	async init() {
		try {
			const user = await api.getCurrentUser();
			state.user = user;
		} catch (error) {
			state.user = null;
		} finally {
			state.loading = false;
		}
	}
};
