export type Provider = 'google' | 'github' | 'apple' | 'microsoft';

export interface User {
  id: string;
  email: string;
  display_name: string | null;
  avatar_url: string | null;
}

export type AuthStatus = 'unknown' | 'anonymous' | 'authed';

interface RefreshResponse {
  access_token: string;
  access_token_expires_in: number;
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
      const body = (await res.json()) as RefreshResponse;
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

  startOAuth(provider: Provider): void {
    window.location.href = `/api/auth/${provider}/start`;
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
