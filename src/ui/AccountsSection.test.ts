import { describe, expect, it } from 'vitest';
import type { FoundAccount } from '../types';
import { foundNote } from './AccountsSection';

const NOW = Date.UTC(2026, 8, 25, 12);
const DAY = 24 * 60 * 60 * 1000;

function found(over: Partial<FoundAccount>): FoundAccount {
  return { name: 'claude-work', harness: 'claude', home: '~/.claude-work', command: null, via: null, state: 'active', signedIn: false, folderExists: true, email: null, lastUsed: null, new: true, ...over };
}

describe('foundNote', () => {
  it('says who is signed in and when it was used', () => {
    expect(foundNote(found({ signedIn: true, email: 'me@example.com', lastUsed: NOW - 3 * DAY }), NOW)).toBe('Signed in as me@example.com, used 3d ago');
    expect(foundNote(found({ lastUsed: NOW - 2 * 60 * 60 * 1000 }), NOW)).toBe('Not signed in, used 2h ago');
  });

  it('says a quiet sign-in has no sessions lately', () => {
    expect(foundNote(found({ state: 'signedIn', signedIn: true }), NOW)).toBe('Signed in, no sessions yet');
    expect(foundNote(found({ state: 'signedIn', signedIn: true, lastUsed: NOW - 60 * DAY }), NOW)).toBe('Signed in, last used 60d ago');
  });

  it('tells an installed harness from a folder that needs a sign-in', () => {
    expect(foundNote(found({ state: 'idle', folderExists: false }), NOW)).toBe('Installed, not set up yet');
    expect(foundNote(found({ state: 'idle' }), NOW)).toBe('Not signed in');
  });

  it('names your wrapper or alias', () => {
    expect(foundNote(found({ state: 'idle', command: 'cw', via: '~/.zshrc' }), NOW)).toBe('Not signed in · starts with cw from ~/.zshrc');
    // A command Codealong made up isn't yours, so it isn't named here.
    expect(foundNote(found({ state: 'idle', command: 'CLAUDE_CONFIG_DIR=~/.claude-work claude' }), NOW)).toBe('Not signed in');
  });
});
