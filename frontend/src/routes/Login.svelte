<script lang="ts">
  import { auth, type Provider } from '../lib/auth.svelte';
  import { router } from '../lib/router.svelte';

  const allProviders: { id: Provider; label: string }[] = [
    { id: 'google', label: 'Continue with Google' },
    { id: 'github', label: 'Continue with GitHub' },
    { id: 'apple', label: 'Continue with Apple' },
    { id: 'microsoft', label: 'Continue with Microsoft' },
  ];
  const providers = $derived(allProviders.filter((p) => auth.providers.includes(p.id)));

  let email = $state('');
  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function submit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await auth.loginWithPassword(email, password);
      router.navigate('/notes');
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="container stack">
  <h1>Sign in</h1>

  {#if error}
    <div class="card" style="border-color: var(--danger); color: var(--danger);">{error}</div>
  {/if}

  <form class="card stack" onsubmit={submit}>
    <label class="stack">
      <span class="muted">Email</span>
      <input type="email" autocomplete="email" required bind:value={email} />
    </label>
    <label class="stack">
      <span class="muted">Password</span>
      <input type="password" autocomplete="current-password" required bind:value={password} />
    </label>
    <div class="row">
      <button class="primary" type="submit" disabled={busy}>
        {busy ? 'Signing in…' : 'Sign in'}
      </button>
      <button type="button" onclick={() => router.navigate('/signup')}>
        Have an invite? Sign up
      </button>
    </div>
  </form>

  {#if providers.length > 0}
    <div class="stack">
      <p class="muted">Or use a provider you've already linked to your account:</p>
      {#each providers as p (p.id)}
        <button onclick={() => auth.startOAuthLogin(p.id)}>{p.label}</button>
      {/each}
    </div>
  {/if}
</div>
