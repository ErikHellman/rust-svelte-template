<script lang="ts">
  import { onMount } from 'svelte';
  import Home from './routes/Home.svelte';
  import Login from './routes/Login.svelte';
  import Notes from './routes/Notes.svelte';
  import { auth } from './lib/auth.svelte';
  import { router } from './lib/router.svelte';

  onMount(() => {
    void auth.bootstrap();
  });
</script>

{#if router.current === '/' || router.current === ''}
  <Home />
{:else if router.current === '/login'}
  <Login />
{:else if router.current === '/notes'}
  <Notes />
{:else}
  <div class="container stack">
    <h1>Not found</h1>
    <p class="muted">No route matches <code>{router.current}</code>.</p>
    <button onclick={() => router.navigate('/')}>Go home</button>
  </div>
{/if}
