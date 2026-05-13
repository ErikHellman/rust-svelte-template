<script lang="ts">
  import { api, ApiError } from '../lib/api';
  import { auth } from '../lib/auth.svelte';
  import { router } from '../lib/router.svelte';

  interface Note {
    id: string;
    title: string;
    body: string;
    created_at: string;
    updated_at: string;
  }

  let notes = $state<Note[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let newTitle = $state('');
  let newBody = $state('');
  let editingId = $state<string | null>(null);
  let editTitle = $state('');
  let editBody = $state('');

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      notes = await api<Note[]>('/notes');
    } catch (e) {
      error = e instanceof ApiError ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function create(): Promise<void> {
    if (!newTitle.trim()) return;
    try {
      const created = await api<Note>('/notes', {
        method: 'POST',
        body: JSON.stringify({ title: newTitle, body: newBody }),
      });
      notes = [created, ...notes];
      newTitle = '';
      newBody = '';
    } catch (e) {
      error = e instanceof ApiError ? e.message : String(e);
    }
  }

  function startEdit(n: Note): void {
    editingId = n.id;
    editTitle = n.title;
    editBody = n.body;
  }

  async function saveEdit(): Promise<void> {
    if (!editingId) return;
    try {
      const updated = await api<Note>(`/notes/${editingId}`, {
        method: 'PUT',
        body: JSON.stringify({ title: editTitle, body: editBody }),
      });
      notes = notes.map((n) => (n.id === updated.id ? updated : n));
      editingId = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : String(e);
    }
  }

  async function remove(id: string): Promise<void> {
    if (!confirm('Delete this note?')) return;
    try {
      await api<void>(`/notes/${id}`, { method: 'DELETE' });
      notes = notes.filter((n) => n.id !== id);
    } catch (e) {
      error = e instanceof ApiError ? e.message : String(e);
    }
  }

  $effect(() => {
    if (auth.status === 'anonymous') {
      router.navigate('/login');
      return;
    }
    if (auth.status === 'authed') {
      void load();
    }
  });
</script>

<div class="container stack">
  <div class="row" style="justify-content: space-between;">
    <h1>Your notes</h1>
    <button onclick={() => auth.logout()}>Sign out</button>
  </div>

  {#if error}
    <div class="card" style="border-color: var(--danger); color: var(--danger);">{error}</div>
  {/if}

  <div class="card stack">
    <input placeholder="Title" bind:value={newTitle} />
    <textarea placeholder="Body (optional)" bind:value={newBody}></textarea>
    <div class="row">
      <button class="primary" onclick={create} disabled={!newTitle.trim()}>Add note</button>
    </div>
  </div>

  {#if loading}
    <p class="muted">loading…</p>
  {:else if notes.length === 0}
    <p class="muted">No notes yet — add your first above.</p>
  {:else}
    <div class="stack">
      {#each notes as n (n.id)}
        <div class="card stack">
          {#if editingId === n.id}
            <input bind:value={editTitle} />
            <textarea bind:value={editBody}></textarea>
            <div class="row">
              <button class="primary" onclick={saveEdit}>Save</button>
              <button onclick={() => (editingId = null)}>Cancel</button>
            </div>
          {:else}
            <strong>{n.title}</strong>
            {#if n.body}
              <div style="white-space: pre-wrap;">{n.body}</div>
            {/if}
            <div class="row muted" style="font-size: 0.85em;">
              <span>updated {new Date(n.updated_at).toLocaleString()}</span>
            </div>
            <div class="row">
              <button onclick={() => startEdit(n)}>Edit</button>
              <button class="danger" onclick={() => remove(n.id)}>Delete</button>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
