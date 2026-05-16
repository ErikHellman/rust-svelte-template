import { beforeEach, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../test/msw-server';
import { auth } from './auth.svelte';

function resetAuth(): void {
  auth.clear();
  auth.providers = [];
  auth.status = 'unknown';
}

beforeEach(resetAuth);

describe('auth.loadProviders', () => {
  it('stores providers reported by the backend', async () => {
    server.use(
      http.get('/api/auth/providers', () => HttpResponse.json({ providers: ['google', 'github'] })),
    );
    await auth.loadProviders();
    expect(auth.providers).toEqual(['google', 'github']);
  });

  it('leaves providers empty if the request fails', async () => {
    server.use(http.get('/api/auth/providers', () => HttpResponse.error()));
    await auth.loadProviders();
    expect(auth.providers).toEqual([]);
  });

  it('leaves providers empty on a non-2xx response', async () => {
    server.use(http.get('/api/auth/providers', () => HttpResponse.json({}, { status: 500 })));
    await auth.loadProviders();
    expect(auth.providers).toEqual([]);
  });
});

describe('auth.refresh', () => {
  it('stores access token and marks status authed on 200', async () => {
    server.use(
      http.post('/api/auth/refresh', () =>
        HttpResponse.json({
          access_token: 'fresh-token',
          access_token_expires_in: 900,
          role: 'user',
        }),
      ),
    );
    const ok = await auth.refresh();
    expect(ok).toBe(true);
    expect(auth.accessToken).toBe('fresh-token');
    expect(auth.status).toBe('authed');
  });

  it('returns false on 401 without changing state', async () => {
    const ok = await auth.refresh();
    expect(ok).toBe(false);
    expect(auth.accessToken).toBeNull();
    expect(auth.status).toBe('unknown');
  });

  it('returns false (and does not throw) when the network errors out', async () => {
    server.use(http.post('/api/auth/refresh', () => HttpResponse.error()));
    await expect(auth.refresh()).resolves.toBe(false);
  });
});

describe('auth.loginWithPassword', () => {
  it('captures the access token and loads the user on success', async () => {
    server.use(
      http.post('/api/auth/login', () =>
        HttpResponse.json({
          access_token: 'login-token',
          access_token_expires_in: 900,
          role: 'admin',
        }),
      ),
      http.get('/api/auth/me', ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer login-token');
        return HttpResponse.json({
          id: 'u1',
          email: 'a@b.com',
          display_name: 'A',
          avatar_url: null,
          role: 'admin',
        });
      }),
    );

    await auth.loginWithPassword('a@b.com', 'longenough1');
    expect(auth.accessToken).toBe('login-token');
    expect(auth.status).toBe('authed');
    expect(auth.user?.email).toBe('a@b.com');
  });

  it('throws with the server-supplied error message on 401', async () => {
    server.use(
      http.post('/api/auth/login', () =>
        HttpResponse.json({ error: 'invalid credentials' }, { status: 401 }),
      ),
    );
    await expect(auth.loginWithPassword('a@b.com', 'wrong')).rejects.toThrow('invalid credentials');
    expect(auth.accessToken).toBeNull();
    expect(auth.status).toBe('unknown');
  });
});

describe('auth.signupWithPassword', () => {
  it('sends the invite code and authenticates on success', async () => {
    let receivedBody: unknown;
    server.use(
      http.post('/api/auth/signup/password', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          access_token: 'signup-token',
          access_token_expires_in: 900,
          role: 'user',
        });
      }),
      http.get('/api/auth/me', () =>
        HttpResponse.json({
          id: 'u2',
          email: 'new@b.com',
          display_name: 'New',
          avatar_url: null,
          role: 'user',
        }),
      ),
    );

    await auth.signupWithPassword({
      code: 'INVITE-1',
      email: 'new@b.com',
      password: 'longenough1',
      display_name: 'New',
    });

    expect(receivedBody).toEqual({
      code: 'INVITE-1',
      email: 'new@b.com',
      password: 'longenough1',
      display_name: 'New',
    });
    expect(auth.accessToken).toBe('signup-token');
    expect(auth.status).toBe('authed');
  });
});

describe('auth.checkInvite', () => {
  it('returns the parsed body on 200', async () => {
    server.use(
      http.post('/api/auth/signup/invite/check', () =>
        HttpResponse.json({ valid: true, bound_email: 'x@y.z', role: 'user' }),
      ),
    );
    const result = await auth.checkInvite('CODE');
    expect(result).toEqual({ valid: true, bound_email: 'x@y.z', role: 'user' });
  });
});

describe('auth.logout', () => {
  it('clears state even if the server returns an error', async () => {
    auth.accessToken = 'stale';
    auth.user = {
      id: 'u',
      email: 'a@b.com',
      display_name: null,
      avatar_url: null,
      role: 'user',
    };
    auth.status = 'authed';
    server.use(http.post('/api/auth/logout', () => HttpResponse.json({}, { status: 500 })));

    await auth.logout();
    expect(auth.accessToken).toBeNull();
    expect(auth.user).toBeNull();
    expect(auth.status).toBe('anonymous');
  });
});

describe('auth.bootstrap', () => {
  it('settles on "anonymous" when refresh fails', async () => {
    await auth.bootstrap();
    expect(auth.status).toBe('anonymous');
    expect(auth.user).toBeNull();
  });

  it('loads providers and the user on a successful refresh', async () => {
    server.use(
      http.get('/api/auth/providers', () => HttpResponse.json({ providers: ['google'] })),
      http.post('/api/auth/refresh', () =>
        HttpResponse.json({
          access_token: 'boot-token',
          access_token_expires_in: 900,
          role: 'admin',
        }),
      ),
      http.get('/api/auth/me', () =>
        HttpResponse.json({
          id: 'u3',
          email: 'boot@b.com',
          display_name: 'Boot',
          avatar_url: null,
          role: 'admin',
        }),
      ),
    );
    await auth.bootstrap();
    expect(auth.providers).toEqual(['google']);
    expect(auth.status).toBe('authed');
    expect(auth.user?.email).toBe('boot@b.com');
  });
});
