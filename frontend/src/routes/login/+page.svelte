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

<div class="login-container">
	<div class="login-box">
		<div class="login-header">
			<span>┌─ LOGIN ─────────────────────┐</span>
		</div>
		<div class="login-content">
			{#if error}
				<div class="error-msg">
					<span>! {error}</span>
				</div>
			{/if}

			<form onsubmit={handleLogin}>
				<div class="field">
					<label for="username">USERNAME:</label>
					<input
						type="text"
						id="username"
						bind:value={username}
						required
						placeholder="enter username"
					/>
				</div>

				<div class="field">
					<label for="password">PASSWORD:</label>
					<input
						type="password"
						id="password"
						bind:value={password}
						required
						placeholder="enter password"
					/>
				</div>

				<button type="submit" disabled={loading} class="submit-btn">
					[{loading ? 'LOGGING IN...' : 'LOGIN'}]
				</button>
			</form>

			<div class="register-link">
				<span class="muted">No account?</span>
				<a href="/register">[REGISTER]</a>
			</div>

			<div class="back-link">
				<a href="/">[BACK TO RADIO]</a>
			</div>
		</div>
		<div class="login-footer">
			<span>└─────────────────────────────┘</span>
		</div>
	</div>
</div>

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		height: 100%;
		background: #0a0a0a;
		color: #e0e0e0;
		font-family: 'Berkeley Mono', 'JetBrains Mono', 'Fira Code', 'SF Mono', monospace;
	}

	.login-container {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}

	.login-box {
		width: 100%;
		max-width: 350px;
	}

	.login-header, .login-footer {
		color: #444;
		font-size: 0.8rem;
	}

	.login-content {
		border-left: 1px solid #333;
		border-right: 1px solid #333;
		padding: 1.5rem;
	}

	.error-msg {
		color: #ff6b6b;
		font-size: 0.8rem;
		margin-bottom: 1rem;
		padding: 0.5rem;
		border: 1px solid #ff6b6b33;
		background: #ff6b6b11;
	}

	.field {
		margin-bottom: 1.25rem;
	}

	.field label {
		display: block;
		color: #666;
		font-size: 0.75rem;
		margin-bottom: 0.4rem;
		letter-spacing: 0.1em;
	}

	.field input {
		width: 100%;
		padding: 0.6rem 0.75rem;
		background: #111;
		border: 1px solid #333;
		color: #e0e0e0;
		font-family: inherit;
		font-size: 0.85rem;
		outline: none;
		transition: border-color 0.15s;
		box-sizing: border-box;
	}

	.field input:focus {
		border-color: #00ff88;
	}

	.field input::placeholder {
		color: #444;
	}

	.submit-btn {
		width: 100%;
		padding: 0.7rem;
		background: transparent;
		border: 1px solid #00ff88;
		color: #00ff88;
		font-family: inherit;
		font-size: 0.85rem;
		cursor: pointer;
		transition: all 0.15s;
		margin-top: 0.5rem;
	}

	.submit-btn:hover:not(:disabled) {
		background: #00ff8822;
	}

	.submit-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.register-link {
		margin-top: 1.5rem;
		text-align: center;
		font-size: 0.8rem;
	}

	.register-link .muted {
		color: #555;
	}

	.register-link a {
		color: #888;
		text-decoration: none;
		margin-left: 0.5rem;
		transition: color 0.15s;
	}

	.register-link a:hover {
		color: #00ff88;
	}

	.back-link {
		margin-top: 1rem;
		text-align: center;
		font-size: 0.75rem;
	}

	.back-link a {
		color: #555;
		text-decoration: none;
		transition: color 0.15s;
	}

	.back-link a:hover {
		color: #888;
	}
</style>
