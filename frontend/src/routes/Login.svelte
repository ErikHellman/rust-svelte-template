<script lang="ts">
  import { auth, type Provider } from '../lib/auth.svelte';

  const providers: { id: Provider; label: string }[] = [
    { id: 'google', label: 'Continue with Google' },
    { id: 'github', label: 'Continue with GitHub' },
    { id: 'apple', label: 'Continue with Apple' },
    { id: 'microsoft', label: 'Continue with Microsoft' },
  ];
</script>

<div class="container stack">
  <h1>Sign in</h1>
  <p class="muted">Choose a provider you've configured in your <code>.env</code>.</p>

  <div class="stack">
    {#each providers as p (p.id)}
      <button onclick={() => auth.startOAuth(p.id)}>{p.label}</button>
    {/each}
  </div>
</div>
