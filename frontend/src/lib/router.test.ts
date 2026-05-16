import { beforeEach, describe, expect, it } from 'vitest';
import { router } from './router.svelte';

beforeEach(() => {
  window.location.hash = '';
});

describe('router', () => {
  it('normalizes an empty hash to "/"', () => {
    window.location.hash = '';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    expect(router.current).toBe('/');
  });

  it('strips the leading "#" from the current path', () => {
    window.location.hash = '#/notes';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    expect(router.current).toBe('/notes');
  });

  it('navigate() sets the location hash with a leading slash', () => {
    router.navigate('notes');
    expect(window.location.hash).toBe('#/notes');
  });

  it('navigate() is idempotent when the hash already matches', () => {
    router.navigate('/login');
    const before = window.location.hash;
    router.navigate('/login');
    expect(window.location.hash).toBe(before);
  });

  it('accepts an input that already starts with "#"', () => {
    router.navigate('#/signup');
    expect(window.location.hash).toBe('#/signup');
  });
});
