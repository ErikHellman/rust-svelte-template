import { beforeEach, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../test/msw-server';
import { api, ApiError } from './api';
import { auth } from './auth.svelte';

function resetAuth(): void {
  auth.clear();
  auth.providers = [];
  auth.status = 'unknown';
}

beforeEach(resetAuth);

describe('api()', () => {
  it('prefixes a bare path with "/api"', async () => {
    server.use(http.get('/api/health', () => HttpResponse.json({ ok: true })));
    await expect(api('/health')).resolves.toEqual({ ok: true });
  });

  it('leaves an already-prefixed path alone', async () => {
    server.use(http.get('/api/notes', () => HttpResponse.json([{ id: 1 }])));
    await expect(api('/api/notes')).resolves.toEqual([{ id: 1 }]);
  });

  it('sends Authorization when an access token is set', async () => {
    auth.accessToken = 'tok-1';
    auth.status = 'authed';
    let seen: string | null = null;
    server.use(
      http.get('/api/notes', ({ request }) => {
        seen = request.headers.get('Authorization');
        return HttpResponse.json([]);
      }),
    );
    await api('/notes');
    expect(seen).toBe('Bearer tok-1');
  });

  it('defaults Content-Type to JSON when a body is present', async () => {
    auth.status = 'authed';
    auth.accessToken = 'tok';
    let seen: string | null = null;
    server.use(
      http.post('/api/notes', ({ request }) => {
        seen = request.headers.get('Content-Type');
        return HttpResponse.json({});
      }),
    );
    await api('/notes', { method: 'POST', body: JSON.stringify({ a: 1 }) });
    expect(seen).toBe('application/json');
  });

  it('returns undefined for a 204 response', async () => {
    server.use(http.delete('/api/notes/1', () => new HttpResponse(null, { status: 204 })));
    await expect(api('/notes/1', { method: 'DELETE' })).resolves.toBeUndefined();
  });

  it('throws ApiError with parsed error + code on non-2xx', async () => {
    server.use(
      http.get('/api/notes', () =>
        HttpResponse.json({ error: 'nope', code: 'forbidden' }, { status: 403 }),
      ),
    );
    await expect(api('/notes')).rejects.toMatchObject({
      status: 403,
      code: 'forbidden',
      message: 'nope',
    });
    await expect(api('/notes')).rejects.toBeInstanceOf(ApiError);
  });

  it('falls back to statusText when the body is not JSON', async () => {
    server.use(
      http.get(
        '/api/notes',
        () => new HttpResponse('plain text', { status: 500, statusText: 'Server Boom' }),
      ),
    );
    await expect(api('/notes')).rejects.toMatchObject({
      status: 500,
      code: 'unknown',
      message: 'Server Boom',
    });
  });

  it('retries once after a 401 when refresh succeeds', async () => {
    auth.accessToken = 'old-token';
    auth.status = 'authed';

    let callCount = 0;
    server.use(
      http.get('/api/notes', ({ request }) => {
        callCount += 1;
        const token = request.headers.get('Authorization');
        if (token === 'Bearer old-token') {
          return HttpResponse.json({ error: 'expired' }, { status: 401 });
        }
        return HttpResponse.json({ retried: true });
      }),
      http.post('/api/auth/refresh', () =>
        HttpResponse.json({
          access_token: 'new-token',
          access_token_expires_in: 900,
          role: 'user',
        }),
      ),
    );

    await expect(api('/notes')).resolves.toEqual({ retried: true });
    expect(callCount).toBe(2);
    expect(auth.accessToken).toBe('new-token');
  });

  it('clears auth and redirects to #/login if refresh fails after a 401', async () => {
    auth.accessToken = 'old-token';
    auth.status = 'authed';
    window.location.hash = '#/notes';

    server.use(
      http.get('/api/notes', () => HttpResponse.json({}, { status: 401 })),
      http.post('/api/auth/refresh', () => HttpResponse.json({}, { status: 401 })),
    );

    await expect(api('/notes')).rejects.toMatchObject({
      status: 401,
      code: 'unauthorized',
    });
    expect(auth.accessToken).toBeNull();
    expect(auth.status).toBe('anonymous');
    expect(window.location.hash).toBe('#/login');
  });

  it('does not attempt refresh when auth is not in the "authed" state', async () => {
    const refreshSpy = vi.fn();
    server.use(
      http.get('/api/notes', () => HttpResponse.json({}, { status: 401 })),
      http.post('/api/auth/refresh', () => {
        refreshSpy();
        return HttpResponse.json({}, { status: 401 });
      }),
    );

    await expect(api('/notes')).rejects.toMatchObject({ status: 401 });
    expect(refreshSpy).not.toHaveBeenCalled();
  });
});
