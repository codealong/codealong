import { describe, expect, it } from 'vitest';
import { isNativeMove, moveDialogTitle, moveDialogWords, moveSubmenuLabel, moveTargetsFor } from './handoff';
import type { AccountInfo, Session } from './types';

const acct = (name: string, harness: AccountInfo['harness'], extra: Partial<AccountInfo> = {}): AccountInfo => ({
  name,
  harness,
  home: `~/.${name}`,
  command: name,
  makesPages: false,
  copiesSettings: false,
  folderExists: true,
  signedIn: true,
  email: null,
  signInNote: null,
  ...extra,
});

const sess = (overrides: Partial<Pick<Session, 'account' | 'harness' | 'resumeCommand' | 'title' | 'status' | 'attachCommand'>> = {}) => ({
  account: 'claude',
  harness: 'claude' as const,
  resumeCommand: 'claude --resume s1',
  title: 'Fix login',
  status: 'idle' as const,
  attachCommand: null,
  ...overrides,
});

describe('moveTargetsFor', () => {
  const accounts = [
    acct('claude', 'claude'),
    acct('codex', 'codex'),
    acct('claude2', 'claude'),
    acct('antigravity', 'antigravity'),
    acct('offline', 'claude', { signedIn: false }),
    acct('off', 'codex', { disabled: true }),
  ];

  it('orders same-harness accounts first and includes all other signed-in enabled providers', () => {
    expect(moveTargetsFor(sess(), accounts)).toEqual([
      { account: 'claude2', harness: 'claude', native: true },
      { account: 'codex', harness: 'codex', native: false },
      { account: 'antigravity', harness: 'antigravity', native: false },
    ]);
  });

  it('offers cross-provider targets for non-Claude harnesses too', () => {
    expect(moveTargetsFor(sess({ account: 'antigravity', harness: 'antigravity', resumeCommand: null }), accounts)).toEqual([
      { account: 'claude', harness: 'claude', native: false },
      { account: 'codex', harness: 'codex', native: false },
      { account: 'claude2', harness: 'claude', native: false },
    ]);
  });
});

describe('isNativeMove and copy', () => {
  const nativeClaude = { account: 'claude2', harness: 'claude' as const, native: true };
  const crossCodex = { account: 'codex', harness: 'codex' as const, native: false };

  it('keeps lossless native path only for same-harness default moves', () => {
    expect(isNativeMove(nativeClaude)).toBe(true);
    expect(isNativeMove(nativeClaude, { keep: true })).toBe(false);
    expect(isNativeMove(nativeClaude, { prompt: 'Write tests' })).toBe(false);
    expect(isNativeMove(nativeClaude, { place: 'codealong' })).toBe(false);
    expect(isNativeMove(crossCodex)).toBe(false);
  });

  it('labels cross-provider submenu items with their provider and highlights room', () => {
    expect(moveSubmenuLabel(nativeClaude, 'claude', 'claude2')).toBe('claude2 (has room)');
    expect(moveSubmenuLabel(crossCodex, 'claude', 'codex')).toBe('codex · Codex (has room)');
    expect(moveSubmenuLabel(crossCodex, 'claude', null)).toBe('codex · Codex');
  });

  it('explains native moves, cross-provider handoffs, and kept copies clearly', () => {
    expect(moveDialogTitle(nativeClaude, 'claude', false)).toBe('Move to another account');
    expect(moveDialogTitle(crossCodex, 'claude', false)).toBe('Move to Codex');
    expect(moveDialogTitle(crossCodex, 'claude', true)).toBe('Hand off to another account');

    expect(moveDialogWords(sess(), nativeClaude, 'Ghostty', true)).toBe(
      'A copy of “Fix login” goes on under claude2 in a new Ghostty tab, told to carry on where it stopped, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess(), crossCodex, 'Ghostty', false, { place: 'codealong' })).toContain(
      'A new Codex session starts under codex in Codealong with a handoff brief and a link to the full transcript, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess({ status: 'working' }), crossCodex, 'Ghostty', false, { place: 'codealong', keep: true })).toContain(
      '; this session stays open.',
    );
  });
});
