<script lang="ts">
	import '../app.css';
	import { authStore } from '$lib/stores/auth.svelte';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';

	let { children } = $props();

	onMount(() => {
		authStore.init();
	});
</script>

{#if authStore.loading}
	<div class="min-h-screen flex items-center justify-center">
		<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
	</div>
{:else}
	<div class="min-h-screen bg-gray-900">
		<!-- Navigation -->
		<nav class="bg-gray-800 border-b border-gray-700">
			<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
				<div class="flex items-center justify-between h-16">
					<div class="flex items-center">
						<a href="/" class="text-xl font-bold text-white">Navidrome Radio</a>
					</div>

					<div class="flex items-center gap-4">
						{#if authStore.isAuthenticated}
							{#if authStore.isAdmin}
								<a
									href="/admin"
									class="text-gray-300 hover:text-white px-3 py-2 rounded-md text-sm font-medium"
								>
									Admin
								</a>
							{/if}
							<span class="text-gray-400 text-sm">{authStore.user?.username}</span>
							<button
								onclick={() => {
									authStore.logout();
									goto('/login');
								}}
								class="bg-red-600 hover:bg-red-700 text-white px-4 py-2 rounded-md text-sm font-medium"
							>
								Logout
							</button>
						{:else}
							<a
								href="/login"
								class="text-gray-300 hover:text-white px-3 py-2 rounded-md text-sm font-medium"
							>
								Login
							</a>
						{/if}
					</div>
				</div>
			</div>
		</nav>

		<!-- Main content -->
		<main>
			{@render children()}
		</main>
	</div>
{/if}
