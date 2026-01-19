<script lang="ts">
	import '../app.css';
	import { authStore } from '$lib/stores/auth.svelte';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';

	let { children } = $props();

	// Check if we're on the homepage
	let isHomePage = $derived($page.url.pathname === '/');

	onMount(() => {
		authStore.init();
	});
</script>

{#if authStore.loading}
	<div class="loading-screen">
		<span class="loading-text">LOADING...</span>
	</div>
{:else if isHomePage}
	<!-- Homepage has its own full layout -->
	{@render children()}
{:else}
	<div class="app-container">
		{@render children()}
	</div>
{/if}

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		height: 100%;
		overflow: hidden;
		background: #0a0a0a;
		color: #e0e0e0;
		font-family: 'Berkeley Mono', 'JetBrains Mono', 'Fira Code', 'SF Mono', monospace;
	}

	.loading-screen {
		height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: #0a0a0a;
	}

	.loading-text {
		color: #00ff88;
		font-size: 1rem;
		animation: blink 1s infinite;
	}

	@keyframes blink {
		50% { opacity: 0.5; }
	}

	.app-container {
		min-height: 100vh;
		background: #0a0a0a;
	}
</style>
