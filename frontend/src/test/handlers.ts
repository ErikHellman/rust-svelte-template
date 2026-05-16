import { http, HttpResponse } from 'msw';

/**
 * Default MSW handlers used by every test. Individual tests can override
 * specific routes via `server.use(...)`; the `afterEach` reset in setup.ts
 * restores these defaults.
 *
 * Defaults model an anonymous, no-OAuth-providers backend so tests start
 * from a known empty state.
 */
export const defaultHandlers = [
  http.get('/api/auth/providers', () => HttpResponse.json({ providers: [] })),
  http.post('/api/auth/refresh', () => HttpResponse.json({}, { status: 401 })),
];
