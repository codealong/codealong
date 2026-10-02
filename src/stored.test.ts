import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { asText, flag, oneOf, readStored, strings, writeStored, type Codec } from './stored';

// What's kept now is what people chose before an update, so these pin the exact strings.
let items: Map<string, string>;
beforeEach(() => {
  items = new Map();
  vi.stubGlobal('window', {
    localStorage: {
      getItem: (k: string) => items.get(k) ?? null,
      setItem: (k: string, v: string) => void items.set(k, v),
      removeItem: (k: string) => void items.delete(k),
    },
  });
});
afterEach(() => vi.unstubAllGlobals());

describe('codecs', () => {
  it('write a flag as 1 or 0 and read only 1 as on', () => {
    writeStored('f', flag, true);
    expect(items.get('f')).toBe('1');
    writeStored('f', flag, false);
    expect(items.get('f')).toBe('0');
    expect(readStored('f', flag, true)).toBe(false);
    items.set('f', 'yes');
    expect(readStored('f', flag, true)).toBe(false);
    items.delete('f');
    expect(readStored('f', flag, true)).toBe(true);
  });

  it('read one of a list, else the fallback', () => {
    const view = oneOf(['sessions', 'worktrees'] as const);
    expect(readStored('v', view, 'sessions')).toBe('sessions');
    writeStored('v', view, 'worktrees');
    expect(items.get('v')).toBe('worktrees');
    expect(readStored('v', view, 'sessions')).toBe('worktrees');
    items.set('v', 'canvas');
    expect(readStored('v', view, 'sessions')).toBe('sessions');
  });

  it('keep text as it is, with a fallback of another type for none', () => {
    expect(readStored('p', asText, null)).toBeNull();
    writeStored('p', asText, 'Acme');
    expect(items.get('p')).toBe('Acme');
    expect(readStored('p', asText, null)).toBe('Acme');
  });

  it('write a list of strings as JSON and drop what else it holds', () => {
    writeStored('l', strings, ['a', 'b']);
    expect(items.get('l')).toBe('["a","b"]');
    items.set('l', '["a",3,null,"b"]');
    expect(readStored('l', strings, [])).toEqual(['a', 'b']);
    items.set('l', '{"a":1}');
    expect(readStored('l', strings, ['x'])).toEqual(['x']);
    items.set('l', 'not json');
    expect(readStored('l', strings, ['x'])).toEqual(['x']);
  });

  it('take the key out when the codec writes null', () => {
    const width: Codec<number | null> = { parse: (raw) => Number(raw) || undefined, format: (w) => (w == null ? null : String(w)) };
    writeStored('w', width, 320);
    expect(items.get('w')).toBe('320');
    writeStored('w', width, null);
    expect(items.has('w')).toBe(false);
  });
});

describe('without storage', () => {
  it('reads the fallback and writes nothing, without failing', () => {
    vi.stubGlobal('window', {
      get localStorage(): Storage {
        throw new Error('The operation is insecure.');
      },
    });
    expect(readStored('f', flag, true)).toBe(true);
    expect(() => writeStored('f', flag, false)).not.toThrow();
  });
});
