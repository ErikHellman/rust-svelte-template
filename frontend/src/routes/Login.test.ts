import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { server } from '../test/msw-server';
import { auth } from '../lib/auth.svelte';
import { router } from '../lib/router.svelte';
import Login from './Login.svelte';

function resetAuth(): void {
  auth.clear();
  auth.providers = [];
  auth.status = 'unknown';
}

beforeEach(() => {
  resetAuth();
  window.location.hash = '';
});

describe('<Login>', () => {
  it('signs in with valid credentials and navigates to /notes', async () => {
    server.use(
      http.post('/api/auth/login', async ({ request }) => {
        const body = (await request.json()) as { email: string; password: string };
        expect(body).toEqual({ email: 'user@example.com', password: 'longenough1' });
        return HttpResponse.json({
          access_token: 'integration-token',
          access_token_expires_in: 900,
          role: 'user',
        });
      }),
      http.get('/api/auth/me', () =>
        HttpResponse.json({
          id: 'u1',
          email: 'user@example.com',
          display_name: 'User',
          avatar_url: null,
          role: 'user',
        }),
      ),
    );

    render(Login);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText(/email/i), 'user@example.com');
    await user.type(screen.getByLabelText(/password/i), 'longenough1');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    await waitFor(() => expect(auth.status).toBe('authed'));
    expect(auth.accessToken).toBe('integration-token');
    expect(auth.user?.email).toBe('user@example.com');
    expect(window.location.hash).toBe('#/notes');
  });

  it('renders the server error and leaves the user signed out on 401', async () => {
    server.use(
      http.post('/api/auth/login', () =>
        HttpResponse.json({ error: 'bad credentials' }, { status: 401 }),
      ),
    );

    render(Login);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText(/email/i), 'user@example.com');
    await user.type(screen.getByLabelText(/password/i), 'wrong-password');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    expect(await screen.findByText(/bad credentials/i)).toBeInTheDocument();
    expect(auth.accessToken).toBeNull();
    expect(auth.status).toBe('unknown');
    expect(window.location.hash).not.toBe('#/notes');
  });

  it('shows OAuth buttons only for configured providers', async () => {
    auth.providers = ['google'];
    render(Login);

    expect(screen.getByRole('button', { name: /continue with google/i })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /continue with github/i })).toBeNull();
  });

  it('navigates to /signup when the "Have an invite?" button is clicked', async () => {
    render(Login);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /have an invite/i }));
    expect(router.current).toBe('/signup');
  });
});
