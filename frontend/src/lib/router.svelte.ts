/** A tiny hash router so the SPA has no extra routing dependency. */
class Router {
  current = $state<string>(normalizeHash(window.location.hash));

  constructor() {
    window.addEventListener('hashchange', () => {
      this.current = normalizeHash(window.location.hash);
    });
  }

  navigate(to: string): void {
    const hash = to.startsWith('#') ? to : `#${to.startsWith('/') ? to : `/${to}`}`;
    if (window.location.hash !== hash) {
      window.location.hash = hash;
    }
  }
}

function normalizeHash(raw: string): string {
  if (!raw || raw === '#' || raw === '#/') return '/';
  return raw.startsWith('#') ? raw.slice(1) : raw;
}

export const router = new Router();
