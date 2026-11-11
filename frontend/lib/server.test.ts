import { describe, expect, test } from 'bun:test';
import { authHeaders, normalizeServerUrl } from './server';

describe('server address handling', () => {
  test('normalizes origins and strips trailing slashes', () => {
    expect(normalizeServerUrl('http://192.168.1.10:8080/')).toBe('http://192.168.1.10:8080');
    expect(normalizeServerUrl('  https://games.example.com/launcher/ ')).toBe(
      'https://games.example.com/launcher',
    );
  });
  test('rejects missing schemes, wrong schemes, and embedded credentials', () => {
    expect(() => normalizeServerUrl('')).toThrow('Enter the server address');
    expect(() => normalizeServerUrl('192.168.1.10:8080')).toThrow('not a valid URL');
    expect(() => normalizeServerUrl('ftp://example.com')).toThrow('http:// or https://');
    expect(() => normalizeServerUrl('http://user:pass@example.com')).toThrow('API key field');
  });
  test('builds bearer headers and requires a key', () => {
    expect(authHeaders('  secret  ')).toEqual({ Authorization: 'Bearer secret' });
    expect(() => authHeaders('   ')).toThrow('API key');
  });
});
