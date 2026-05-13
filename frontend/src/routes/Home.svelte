<script lang="ts">
  import { auth } from '../lib/auth.svelte';
  import { router } from '../lib/router.svelte';
</script>

<div class="container stack">
  <h1>Full-stack template</h1>
  <p class="muted">
    Rust (Axum + sqlx + SQLite) on the back, Svelte 5 on the front. Use this as a starting point for
    new projects: clone, set your env, and ship.
  </p>

  {#if auth.status === 'unknown'}
    <p class="muted">checking session…</p>
  {:else if auth.status === 'authed'}
    <p>
      Signed in as <strong>{auth.user?.display_name ?? auth.user?.email}</strong>.
    </p>
    <div class="row">
      <button class="primary" onclick={() => router.navigate('/notes')}>Open notes</button>
      <button onclick={() => auth.logout()}>Sign out</button>
    </div>
  {:else}
    <div class="row">
      <button class="primary" onclick={() => router.navigate('/login')}>Sign in</button>
    </div>
  {/if}
</div>
