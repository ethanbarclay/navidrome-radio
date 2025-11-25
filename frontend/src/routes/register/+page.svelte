<script lang="ts">
	import { authStore } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';

	let username = $state('');
	let email = $state('');
	let password = $state('');
	let confirmPassword = $state('');
	let error = $state<string | null>(null);
	let loading = $state(false);

	async function handleRegister(e: Event) {
		e.preventDefault();
		loading = true;
		error = null;

		if (password !== confirmPassword) {
			error = 'Passwords do not match';
			loading = false;
			return;
		}

		if (password.length < 8) {
			error = 'Password must be at least 8 characters';
			loading = false;
			return;
		}

		try {
			await authStore.register(username, email, password);
			goto('/');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Registration failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="min-h-[calc(100vh-4rem)] flex items-center justify-center px-4">
	<div class="max-w-md w-full">
		<div class="bg-gray-800 rounded-lg shadow-xl p-8">
			<h2 class="text-3xl font-bold text-center mb-8">Register</h2>

			{#if error}
				<div class="bg-red-500/10 border border-red-500 text-red-500 px-4 py-3 rounded mb-6">
					{error}
				</div>
			{/if}

			<form onsubmit={handleRegister}>
				<div class="mb-6">
					<label for="username" class="block text-sm font-medium text-gray-300 mb-2">
						Username
					</label>
					<input
						type="text"
						id="username"
						bind:value={username}
						required
						minlength="3"
						class="w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="Choose a username"
					/>
				</div>

				<div class="mb-6">
					<label for="email" class="block text-sm font-medium text-gray-300 mb-2"> Email </label>
					<input
						type="email"
						id="email"
						bind:value={email}
						required
						class="w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="your@email.com"
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
						minlength="8"
						class="w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="At least 8 characters"
					/>
				</div>

				<div class="mb-6">
					<label for="confirmPassword" class="block text-sm font-medium text-gray-300 mb-2">
						Confirm Password
					</label>
					<input
						type="password"
						id="confirmPassword"
						bind:value={confirmPassword}
						required
						class="w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white"
						placeholder="Confirm your password"
					/>
				</div>

				<button
					type="submit"
					disabled={loading}
					class="w-full bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-semibold py-3 px-4 rounded-lg transition-colors"
				>
					{loading ? 'Creating account...' : 'Register'}
				</button>
			</form>

			<p class="mt-6 text-center text-gray-400 text-sm">
				Already have an account?
				<a href="/login" class="text-blue-400 hover:text-blue-300">Login</a>
			</p>
		</div>
	</div>
</div>
