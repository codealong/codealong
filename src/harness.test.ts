import { describe, expect, it } from 'vitest';
import { harnessOfKey, nameShown, renames, sessionName, usageNote } from './harness';
import type { Usage } from './types';

describe('usageNote', () => {
  const usage = (u: Partial<Usage>): Usage => ({ account: 'a', harness: 'antigravity', plan: null, meters: [], asOf: null, error: null, needsSignIn: false, loading: false, ...u });
  it('says no limits only where the engine says its harness keeps none', () => {
    expect(usageNote(usage({ harness: 'opencode', noLimits: true, error: 'OpenCode bills the provider you bring.' }))).toBe('No limits to show');
    // Antigravity's quotas are read through its command line, so a read that failed says so.
    expect(usageNote(usage({ error: "Couldn't read usage: `agy` didn't answer in 30 seconds" }))).toBe('Usage unavailable');
    expect(usageNote(usage({ loading: true }))).toBe('Reading…');
    expect(usageNote(usage({ needsSignIn: true }))).toBe('Not signed in');
    expect(usageNote(usage({ harness: 'codex' }))).toBe('No turns yet');
    expect(usageNote(undefined)).toBe('No usage yet');
  });
});

describe('sessionName', () => {
  it('makes a name one line, as the engine does', () => {
    expect(sessionName('  Fix the\nlogin   redirect\t')).toBe('Fix the login redirect');
    expect(sessionName('a\u{200b}b\u{202e}c\u{feff}')).toBe('a b c');
    expect(sessionName('x'.repeat(250))).toHaveLength(200);
    expect(sessionName(`${'x'.repeat(199)} y`)).toBe('x'.repeat(199));
    expect(sessionName(' \n\u{200b} ')).toBe('');
  });
});

describe('nameShown', () => {
  it('shows a name as the board does', () => {
    expect(nameShown('Use `vitest`')).toBe("Use 'vitest'");
    expect(nameShown('y'.repeat(90))).toBe('y'.repeat(90));
    expect(nameShown(`${'y'.repeat(88)} zz`)).toBe(`${'y'.repeat(88)}…`);
  });
});

describe('renames', () => {
  it('names what Claude Code, Codex and OpenCode keep, here and on other devices', () => {
    expect(harnessOfKey('@k3j9:codex:codex:0199')).toBe('codex');
    expect(harnessOfKey('nothing')).toBeNull();
    expect(renames({ key: 'claude:claude2:9f62' })).toBe(true);
    expect(renames({ key: 'opencode:opencode:ses_1' })).toBe(true);
    expect(renames({ key: '@k3j9:codex:codex:0199' })).toBe(true);
    // Codex refuses an archived thread, and Antigravity's conversations aren't Codealong's to name.
    expect(renames({ key: 'codex:codex:0199', archived: true })).toBe(false);
    expect(renames({ key: 'antigravity:antigravity:c1' })).toBe(false);
  });
});
