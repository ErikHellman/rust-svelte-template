<script lang="ts">
  import { auth, type Provider } from '../lib/auth.svelte';
  import { router } from '../lib/router.svelte';

  const providers: { id: Provider; label: string }[] = [
    { id: 'google', label: 'Sign up with Google' },
    { id: 'github', label: 'Sign up with GitHub' },
    { id: 'apple', label: 'Sign up with Apple' },
    { id: 'microsoft', label: 'Sign up with Microsoft' },
  ];

  let step = $state<'invite' | 'method'>('invite');
  let code = $state('');
  let boundEmail = $state<string | null>(null);
  let invitedRole = $state<string>('user');

  let email = $state('');
  let password = $state('');
  let displayName = $state('');

  let busy = $state(false);
  let error = $state<string | null>(null);

  // Surface any error that the OAuth callback redirected us back with.
  $effect(() => {
    const search = window.location.hash.split('?')[1];
    if (!search) return;
    const params = new URLSearchParams(search);
    const err = params.get('error');
    if (err) error = err;
  });

  async function checkInvite(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const r = await auth.checkInvite(code.trim());
      if (!r.valid) {
        error = "That invite code isn't valid (or has already been used).";
        return;
      }
      boundEmail = r.bound_email;
      invitedRole = r.role;
      if (boundEmail) email = boundEmail;
      step = 'method';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function submitPassword(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await auth.signupWithPassword({
        code: code.trim(),
        email,
        password,
        display_name: displayName || undefined,
      });
      router.navigate('/notes');
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="container stack">
  <h1>Create an account</h1>

  {#if error}
    <div class="card" style="border-color: var(--danger); color: var(--danger);">{error}</div>
  {/if}

  {#if step === 'invite'}
    <form class="card stack" onsubmit={checkInvite}>
      <p class="muted">Registration is invite-only. Enter the code you were given.</p>
      <label class="stack">
        <span class="muted">Invite code</span>
        <input required autocomplete="off" placeholder="e.g. BOOTSTRAP" bind:value={code} />
      </label>
      <div class="row">
        <button class="primary" type="submit" disabled={busy || !code.trim()}>
          {busy ? 'Checking…' : 'Continue'}
        </button>
        <button type="button" onclick={() => router.navigate('/login')}>
          I already have an account
        </button>
      </div>
    </form>
  {:else}
    <div class="card stack">
      <p class="muted">
        Invite accepted{invitedRole === 'admin' ? ' (admin)' : ''}. Pick how you'd like to sign in.
      </p>

      <form class="stack" onsubmit={submitPassword}>
        <strong>Email &amp; password</strong>
        <label class="stack">
          <span class="muted">Email</span>
          <input
            type="email"
            autocomplete="email"
            required
            bind:value={email}
            readonly={boundEmail !== null}
          />
          {#if boundEmail}
            <span class="muted" style="font-size: 0.85em;">
              This invite is bound to {boundEmail}.
            </span>
          {/if}
        </label>
        <label class="stack">
          <span class="muted">Password (at least 8 characters)</span>
          <input
            type="password"
            autocomplete="new-password"
            required
            minlength="8"
            bind:value={password}
          />
        </label>
        <label class="stack">
          <span class="muted">Display name (optional)</span>
          <input type="text" autocomplete="name" bind:value={displayName} />
        </label>
        <div class="row">
          <button class="primary" type="submit" disabled={busy}>
            {busy ? 'Creating…' : 'Create account'}
          </button>
        </div>
      </form>
    </div>

    <div class="card stack">
      <strong>Or use a provider</strong>
      {#each providers as p (p.id)}
        <button onclick={() => auth.startOAuthSignup(p.id, code.trim())}>{p.label}</button>
      {/each}
    </div>
  {/if}
</div>
