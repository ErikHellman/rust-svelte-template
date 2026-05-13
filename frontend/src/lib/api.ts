import { auth } from './auth.svelte';

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  constructor(message: string, status: number, code: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

interface ErrorBody {
  error?: string;
  code?: string;
}

async function readError(res: Response): Promise<ApiError> {
  let body: ErrorBody = {};
  try {
    body = (await res.json()) as ErrorBody;
  } catch {
    /* not json */
  }
  return new ApiError(
    body.error ?? res.statusText ?? 'request failed',
    res.status,
    body.code ?? 'unknown',
  );
}

function authHeaders(init?: RequestInit): HeadersInit {
  const headers = new Headers(init?.headers);
  if (auth.accessToken) {
    headers.set('Authorization', `Bearer ${auth.accessToken}`);
  }
  if (init?.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  return headers;
}

/**
 * Calls /api/<path>. Adds the Bearer access token, retries once after a 401 by
 * refreshing the access token, and parses JSON on success. Throws ApiError on
 * non-2xx responses.
 */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const url = path.startsWith('/api') ? path : `/api${path.startsWith('/') ? path : `/${path}`}`;
  const send = (): Promise<Response> =>
    fetch(url, { ...init, headers: authHeaders(init), credentials: 'include' });

  let res = await send();
  if (res.status === 401 && auth.status === 'authed') {
    const refreshed = await auth.refresh();
    if (refreshed) {
      res = await send();
    } else {
      auth.clear();
      window.location.hash = '#/login';
      throw new ApiError('session expired', 401, 'unauthorized');
    }
  }
  if (!res.ok) {
    throw await readError(res);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}
