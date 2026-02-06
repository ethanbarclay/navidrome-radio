import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		proxy: {
			'/api': {
				target: 'http://localhost:8000',
				changeOrigin: true
			}
		}
	},
	// Required for Three.js SSR compatibility
	ssr: {
		noExternal: ['three']
	},
	// Optimize Three.js imports
	optimizeDeps: {
		include: ['three']
	}
});
