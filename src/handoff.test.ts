import { describe, expect, it } from 'vitest';
import { handoffActionLabel, handoffPlaceholder, handoffSubmitOptions, handoffSummary, isNativeMove, moveDialogWords, moveSubmenuLabel, moveTargetsFor } from './handoff';
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

describe('handoff copy and submit options', () => {
  const nativeClaude = { account: 'claude2', harness: 'claude' as const, native: true };
  const codex = { account: 'codex', harness: 'codex' as const, native: false };

  it('formats submenu labels with provider name on cross-provider targets', () => {
    expect(moveSubmenuLabel(nativeClaude, 'claude', null)).toBe('claude2');
    expect(moveSubmenuLabel(nativeClaude, 'claude', 'claude2')).toBe('claude2 (has room)');
    expect(moveSubmenuLabel(codex, 'claude', null)).toBe('codex · Codex');
    expect(moveSubmenuLabel(codex, 'claude', 'codex')).toBe('codex · Codex (has room)');
  });

  it('describes native moves, background jobs, working turns, cross-provider handoffs, and keep-open copies', () => {
    expect(moveDialogWords(sess(), nativeClaude, 'Ghostty', false)).toBe(
      'A copy of “Fix login” goes on under claude2 in a new Ghostty tab, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess({ attachCommand: 'claude --attach x' }), nativeClaude, 'Ghostty', true)).toBe(
      'A copy of “Fix login” goes on under claude2 as a background job, told to carry on where it stopped, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess({ status: 'working' }), nativeClaude, 'Ghostty', false)).toMatch(/ Its turn is stopped first\.$/);
    expect(moveDialogWords(sess(), codex, 'Ghostty', false, { place: 'codealong' })).toBe(
      'A new Codex session starts under codex in Codealong with a handoff brief and a link to the full transcript, and this one is closed and archived.',
    );
    expect(moveDialogWords(sess(), codex, 'Ghostty', false, { keep: true, place: 'terminal', prompt: 'Finish the PR' })).toBe(
      'A new Codex session starts under codex in a new Ghostty tab with a handoff brief and a link to the full transcript with your next-step note; this session stays open.',
    );
  });

  it('builds crisp inline summary, placeholder, action label, and submit options', () => {
    expect(handoffSummary(sess(), nativeClaude, false)).toBe('Move to claude2 · closes this');
    expect(handoffSummary(sess(), nativeClaude, true)).toBe('Move to claude2 · carries on · closes this');
    expect(handoffSummary(sess(), codex, false)).toBe('Move to codex (Codex) · closes this');
    expect(handoffSummary(sess(), codex, false, { keep: true })).toBe('Hand off to codex (Codex) · keeps this open');
    expect(handoffPlaceholder(codex, false)).toBe('Add a note for codex, or press ⏎ to move…');
    expect(handoffPlaceholder(codex, true)).toBe('Add a note for codex, or press ⏎ to hand off…');
    expect(handoffActionLabel('codex', false)).toBe('Move to codex');
    expect(handoffActionLabel('codex', true)).toBe('Hand off to codex');
    expect(handoffSubmitOptions(nativeClaude, { account: 'claude2', model: null, effort: null, place: null, keep: false }, '', true)).toBeUndefined();
    expect(handoffSubmitOptions(codex, { account: 'codex', model: 'gpt-5.5', effort: 'high', place: 'codealong', keep: true }, '  Fix test  ', true)).toEqual({
      keep: true,
      place: 'codealong',
      model: 'gpt-5.5',
      effort: 'high',
      prompt: 'Fix test',
    });
    // Places that don't take a model omit model & effort cleanly.
    expect(handoffSubmitOptions(codex, { account: 'codex', model: 'gpt-5.5', effort: 'high', place: 'claude-app', keep: false }, '', false)).toEqual({
      keep: undefined,
      place: 'claude-app',
      model: undefined,
      effort: undefined,
      prompt: undefined,
    });
  });
});
