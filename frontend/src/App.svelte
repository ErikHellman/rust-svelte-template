<script lang="ts">
  import { onMount } from 'svelte';
  import Home from './routes/Home.svelte';
  import Login from './routes/Login.svelte';
  import Notes from './routes/Notes.svelte';
  import Signup from './routes/Signup.svelte';
  import { auth } from './lib/auth.svelte';
  import { router } from './lib/router.svelte';

  onMount(() => {
    void auth.bootstrap();
  });

  // The hash router stores the raw path, including query string (e.g.
  // "/signup?error=…"). Split that off when picking a route component.
  const route = $derived(router.current.split('?')[0]);
</script>

{#if route === '/' || route === ''}
  <Home />
{:else if route === '/login'}
  <Login />
{:else if route === '/signup'}
  <Signup />
{:else if route === '/notes'}
  <Notes />
{:else}
  <div class="container stack">
    <h1>Not found</h1>
    <p class="muted">No route matches <code>{route}</code>.</p>
    <button onclick={() => router.navigate('/')}>Go home</button>
  </div>
{/if}
