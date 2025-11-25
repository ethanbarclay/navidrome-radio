<script lang="ts">
	import { authStore } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';

	let username = $state('');
	let password = $state('');
	let error = $state<string | null>(null);
	let loading = $state(false);

	async function handleLogin(e: Event) {
		e.preventDefault();
		loading = true;
		error = null;

		try {
			await authStore.login(username, password);
			goto('/');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Login failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="min-h-[calc(100vh-4rem)] flex items-center justify-center px-4">
	<div class="max-w-md w-full">
		<div class="bg-gray-800 rounded-lg shadow-xl p-8">
			<h2 class="text-3xl font-bold text-center mb-8">Login</h2>

			{#if error}
				<div class="bg-red-500/10 border border-red-500 text-red-500 px-4 py-3 rounded mb-6">
					{error}
				</div>
			{/if}

			<form onsubmit={handleLogin}>
				<div class="mb-6">
					<label for="username" class="block text-sm font-medium text-gray-300 mb-2">
						Username
					</label>
					<input
						type="text"
						id="username"
						bind:value={username}
						required
						class="w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="Enter your username"
					/>
				</div>

				<div class="mb-6">
					<label for="password" class="block text-sm font-medium text-gray-300 mb-2">
						Password
					</label>
					<input
						type="password"
						id="password"
						bind:value={password}
						required
						class="w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="Enter your password"
					/>
				</div>

				<button
					type="submit"
					disabled={loading}
					class="w-full bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-semibold py-3 px-4 rounded-lg transition-colors"
				>
					{loading ? 'Logging in...' : 'Login'}
				</button>
			</form>

			<p class="mt-6 text-center text-gray-400 text-sm">
				Don't have an account?
				<a href="/register" class="text-blue-400 hover:text-blue-300">Register</a>
			</p>
		</div>
	</div>
</div>
