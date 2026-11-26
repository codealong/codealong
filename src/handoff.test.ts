import { describe, expect, it } from 'vitest';
import { findMovedCopy, forkOriginKeys, isNativeMove, moveDialogTitle, moveDialogWords, moveSubmenuLabel, moveTargetsFor } from './handoff';
import * as fx from './testing/fixtures';
import type { AccountInfo, Session } from './types';

const acct = (name: string, harness: AccountInfo['harness'], over: Partial<AccountInfo> = {}): AccountInfo => ({
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
  ...over,
});

const sess = (over: Partial<Session> = {}): Pick<Session, 'account' | 'harness' | 'resumeCommand' | 'title' | 'status' | 'attachCommand'> => ({
  account: 'claude',
  harness: 'claude',
  resumeCommand: 'claude --resume abc',
  title: 'Fix login',
  status: 'idle',
  attachCommand: null,
  ...over,
});

describe('moveTargetsFor', () => {
  it('puts same-harness accounts first and includes signed-in cross-provider accounts', () => {
    const accounts = [
      acct('claude', 'claude'),
      acct('codex', 'codex'),
      acct('claude2', 'claude'),
      acct('antigravity', 'antigravity'),
      acct('cursor', 'cursor', { signedIn: false }),
      acct('opencode', 'opencode', { disabled: true }),
    ];
    expect(moveTargetsFor(sess(), accounts)).toEqual([
      { account: 'claude2', harness: 'claude', native: true },
      { account: 'codex', harness: 'codex', native: false },
      { account: 'antigravity', harness: 'antigravity', native: false },
    ]);
  });

  it('offers cross-provider handoff for non-Claude sessions without native moves', () => {
    const accounts = [acct('claude', 'claude'), acct('codex', 'codex'), acct('antigravity', 'antigravity')];
    expect(moveTargetsFor(sess({ account: 'antigravity', harness: 'antigravity', resumeCommand: null }), accounts)).toEqual([
      { account: 'claude', harness: 'claude', native: false },
      { account: 'codex', harness: 'codex', native: false },
    ]);
  });
});

describe('isNativeMove', () => {
  const nativeClaude = { account: 'claude2', harness: 'claude' as const, native: true };
  const codex = { account: 'codex', harness: 'codex' as const, native: false };

  it('uses native move only for default same-harness Claude moves', () => {
    expect(isNativeMove(nativeClaude)).toBe(true);
    expect(isNativeMove(nativeClaude, {})).toBe(true);
    expect(isNativeMove(nativeClaude, { keep: true })).toBe(false);
    expect(isNativeMove(nativeClaude, { model: 'sonnet' })).toBe(false);
    expect(isNativeMove(nativeClaude, { effort: 'high' })).toBe(false);
    expect(isNativeMove(nativeClaude, { place: 'codealong' })).toBe(false);
    expect(isNativeMove(nativeClaude, { prompt: 'Focus on tests' })).toBe(false);
    expect(isNativeMove(codex)).toBe(false);
  });
});

describe('handoff copy', () => {
  const nativeClaude = { account: 'claude2', harness: 'claude' as const, native: true };
  const codex = { account: 'codex', harness: 'codex' as const, native: false };

  it('formats submenu labels with provider name on cross-provider targets', () => {
    expect(moveSubmenuLabel(nativeClaude, 'claude', null)).toBe('claude2');
    expect(moveSubmenuLabel(nativeClaude, 'claude', 'claude2')).toBe('claude2 (has room)');
    expect(moveSubmenuLabel(codex, 'claude', null)).toBe('codex · Codex');
    expect(moveSubmenuLabel(codex, 'claude', 'codex')).toBe('codex · Codex (has room)');
  });

  it('explains native moves, cross-provider handoffs, picked models, and keep-open copies', () => {
    expect(moveDialogTitle(nativeClaude, 'claude', false)).toBe('Move to another account');
    expect(moveDialogTitle(codex, 'claude', false)).toBe('Move to Codex');
    expect(moveDialogTitle(codex, 'claude', true)).toBe('Hand off to another account');
    expect(moveDialogWords(sess(), nativeClaude, 'Ghostty', false)).toBe(
      'A copy of “Fix login” goes on under claude2 in a new Ghostty tab with its full conversation carried over, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess({ attachCommand: 'claude --attach x' }), nativeClaude, 'Ghostty', true)).toBe(
      'A copy of “Fix login” goes on under claude2 as a background job, told to carry on where it stopped with its full conversation carried over, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess({ status: 'working' }), nativeClaude, 'Ghostty', false)).toMatch(/ Its turn is stopped first\.$/);
    expect(moveDialogWords(sess(), codex, 'Ghostty', false, { model: 'gpt-5.5' }, 'GPT-5.5')).toBe(
      'Starts a new Codex session under codex on GPT-5.5 with a handoff brief and a link to the full transcript on disk, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess(), codex, 'Ghostty', false, { keep: true, model: 'gpt-5.5' }, 'GPT-5.5')).toBe(
      'Starts a new Codex session under codex on GPT-5.5 with a handoff brief and a link to the full transcript on disk; this session stays open.',
    );
  });
});

describe('findMovedCopy', () => {
  it('matches an exact key or a newly arrived fork origin while ignoring older forks', () => {
    const from = 'claude:a:1';
    const oldFork = fx.session({ key: 'codex:b:old', origin: { key: from, how: 'fork' }, startedAt: 100 });
    const newCopy = fx.session({ key: 'codex:b:new', origin: { key: from, how: 'fork' }, startedAt: 200 });
    const lane = fx.lane({ id: '/r/app', sessions: [oldFork, newCopy] });

    const before = forkOriginKeys([fx.lane({ id: '/r/app', sessions: [oldFork] })], from);
    expect([...before]).toEqual(['codex:b:old']);

    expect(findMovedCopy([lane], from, 'codex:b:new', before)?.session.key).toBe('codex:b:new');
    expect(findMovedCopy([lane], from, 'codex:b:missing', before)).toBeNull();
    expect(findMovedCopy([lane], from, null, before)?.session.key).toBe('codex:b:new');
    expect(findMovedCopy([lane], from, null, new Set(['codex:b:old', 'codex:b:new']))).toBeNull();
  });
});
