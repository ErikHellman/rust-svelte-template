export type Provider = 'google' | 'github' | 'apple' | 'microsoft';

export interface User {
  id: string;
  email: string;
  display_name: string | null;
  avatar_url: string | null;
  role: string;
}

export type AuthStatus = 'unknown' | 'anonymous' | 'authed';

interface SessionResponse {
  access_token: string;
  access_token_expires_in: number;
  role: string;
}

interface ErrorBody {
  error?: string;
  code?: string;
}

async function readError(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as ErrorBody;
    return body.error ?? res.statusText;
  } catch {
    return res.statusText;
  }
}

class AuthStore {
  user = $state<User | null>(null);
  accessToken = $state<string | null>(null);
  status = $state<AuthStatus>('unknown');

  async bootstrap(): Promise<void> {
    const ok = await this.refresh();
    if (!ok) {
      this.status = 'anonymous';
      return;
    }
    await this.loadMe();
  }

  /** Calls /api/auth/refresh once. Returns true on success. Never throws. */
  async refresh(): Promise<boolean> {
    try {
      const res = await fetch('/api/auth/refresh', {
        method: 'POST',
        credentials: 'include',
      });
      if (!res.ok) return false;
      const body = (await res.json()) as SessionResponse;
      this.accessToken = body.access_token;
      this.status = 'authed';
      return true;
    } catch {
      return false;
    }
  }

  async loadMe(): Promise<void> {
    if (!this.accessToken) return;
    const res = await fetch('/api/auth/me', {
      headers: { Authorization: `Bearer ${this.accessToken}` },
    });
    if (res.ok) {
      this.user = (await res.json()) as User;
    } else {
      this.clear();
    }
  }

  startOAuthLogin(provider: Provider): void {
    window.location.href = `/api/auth/${provider}/start`;
  }

  startOAuthSignup(provider: Provider, code: string): void {
    const url = `/api/auth/${provider}/signup/start?code=${encodeURIComponent(code)}`;
    window.location.href = url;
  }

  async loginWithPassword(email: string, password: string): Promise<void> {
    const res = await fetch('/api/auth/login', {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password }),
    });
    if (!res.ok) throw new Error(await readError(res));
    const body = (await res.json()) as SessionResponse;
    this.accessToken = body.access_token;
    this.status = 'authed';
    await this.loadMe();
  }

  async signupWithPassword(args: {
    code: string;
    email: string;
    password: string;
    display_name?: string;
  }): Promise<void> {
    const res = await fetch('/api/auth/signup/password', {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(args),
    });
    if (!res.ok) throw new Error(await readError(res));
    const body = (await res.json()) as SessionResponse;
    this.accessToken = body.access_token;
    this.status = 'authed';
    await this.loadMe();
  }

  async checkInvite(code: string): Promise<{
    valid: boolean;
    bound_email: string | null;
    role: string;
  }> {
    const res = await fetch('/api/auth/signup/invite/check', {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
    });
    if (!res.ok) throw new Error(await readError(res));
    return await res.json();
  }

  async logout(): Promise<void> {
    try {
      await fetch('/api/auth/logout', {
        method: 'POST',
        credentials: 'include',
      });
    } finally {
      this.clear();
    }
  }

  clear(): void {
    this.user = null;
    this.accessToken = null;
    this.status = 'anonymous';
  }
}

export const auth = new AuthStore();
