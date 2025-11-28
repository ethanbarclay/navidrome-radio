<script lang="ts">
	import '../../../app.css';
	import { authStore } from '$lib/stores/auth.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();

	onMount(() => {
		authStore.init();
		// Prevent scrolling on the player page
		document.documentElement.style.overflow = 'hidden';
		document.body.style.overflow = 'hidden';
		document.documentElement.style.height = '100%';
		document.body.style.height = '100%';

		return () => {
			// Restore scrolling when leaving this layout
			document.documentElement.style.overflow = '';
			document.body.style.overflow = '';
			document.documentElement.style.height = '';
			document.body.style.height = '';
		};
	});
</script>

<!-- No navigation - full screen player layout that bypasses parent layouts -->
{#if authStore.loading}
	<div class="h-screen flex items-center justify-center bg-gray-900">
		<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
	</div>
{:else}
	{@render children()}
{/if}
