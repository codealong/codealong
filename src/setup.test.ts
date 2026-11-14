import { describe, expect, it } from 'vitest';
import { accountsAfter, configAfter, emblemDrawers, emblemsAfter, firstPicks, folderNote, foldersPicked, typedFolder, type SetupPicks } from './setup';
import type { AccountCfg, Config, EmblemsCfg, SetupAccount, SetupFolder, SetupView } from './types';

const account = (name: string, home: string, more: Partial<SetupAccount> = {}): SetupAccount => ({
  name,
  harness: 'claude',
  home,
  command: null,
  via: null,
  state: 'active',
  signedIn: true,
  folderExists: true,
  email: null,
  signInNote: null,
  lastUsed: null,
  new: true,
  listed: false,
  draws: false,
  ...more,
});

const OFF: EmblemsCfg = { kind: 'off', account: '', prompt: '' };
const picked = (more: Partial<SetupPicks> = {}): SetupPicks => ({ accounts: [], folders: [], terminal: 'ghostty', editor: 'cursor', agents: true, atLogin: false, emblems: OFF, ...more });

const folder = (path: string, more: Partial<SetupFolder> = {}): SetupFolder => ({ path, exists: true, repos: 3, worked: 0, scanned: false, ...more });

const cfg = (accounts: AccountCfg[]): Config => ({ scan: ['~/projects'], apps: { terminal: 'ghostty', editor: 'cursor' }, accounts }) as unknown as Config;

describe('the setup wizard', () => {
  it('starts with the listed accounts and the ones in use or signed in ticked', () => {
    const view: SetupView = {
      accounts: [account('work', '~/.claude-work', { listed: true, state: 'idle' }), account('claude', '~/.claude'), account('codex', '~/.codex', { harness: 'codex', state: 'signedIn' }), account('old', '~/.claude-old', { state: 'idle' })],
      folders: [],
    };
    const picks = firstPicks(view, cfg([]), [{ id: 'ghostty', name: 'Ghostty' }], [{ id: 'vscode', name: 'VS Code' }]);
    expect(picks.accounts).toEqual(['~/.claude-work', '~/.claude', '~/.codex']);
    expect([picks.terminal, picks.editor]).toEqual(['ghostty', 'vscode']);
    expect([picks.agents, picks.atLogin]).toEqual([true, false]);
    expect(picks.emblems).toEqual(OFF);
  });

  it('starts with the emblems config.toml has, so going through it again keeps them', () => {
    const view: SetupView = { accounts: [], folders: [] };
    const emblems: EmblemsCfg = { kind: 'drawn', account: 'codex', prompt: 'Pixel art.' };
    expect(firstPicks(view, { ...cfg([]), emblems }, [], []).emblems).toEqual(emblems);
  });

  it('offers drawn emblems for the accounts kept that can draw, by the names config.toml gets', () => {
    const view: SetupView = {
      accounts: [account('codex', '~/.codex', { harness: 'codex', draws: true, listed: true }), account('claude', '~/.claude'), account('codex-old', '~/.codex-old', { harness: 'codex', signedIn: false }), account('codex-2', '~/.codex-2', { harness: 'codex', draws: true })],
      folders: [],
    };
    expect(emblemDrawers(view, ['~/.codex', '~/.claude', '~/.codex-old', '~/.codex-2'])).toEqual(['codex', 'codex-2']);
    expect(emblemDrawers(view, ['~/.claude', '~/.codex-old'])).toEqual([]);
  });

  it('has drawn emblems drawn by an account kept that can draw', () => {
    const drawn: EmblemsCfg = { kind: 'drawn', account: '', prompt: '' };
    expect(emblemsAfter(drawn, ['codex', 'codex-2']).account).toBe('codex');
    expect(emblemsAfter({ ...drawn, account: 'codex-2' }, ['codex', 'codex-2']).account).toBe('codex-2');
    expect(emblemsAfter({ ...drawn, account: 'codex-old' }, ['codex-2']).account).toBe('codex-2');
    // With none to draw them it's left as picked, and the engine says so in a notice.
    expect(emblemsAfter(drawn, [])).toEqual(drawn);
    // Emoji and none name no account.
    expect(emblemsAfter({ kind: 'emoji', account: '', prompt: '' }, ['codex'])).toEqual({ kind: 'emoji', account: '', prompt: '' });
  });

  it('ticks the folders searched or worked in, else every one with repos', () => {
    expect(foldersPicked([folder('~/projects', { scanned: true }), folder('~/gone', { scanned: true, exists: false }), folder('~/work', { worked: 2 }), folder('~/code')])).toEqual(['~/projects', '~/work']);
    expect(foldersPicked([folder('~/projects', { scanned: true, exists: false }), folder('~/code'), folder('~/src', { repos: 0 })])).toEqual(['~/code']);
  });

  it('keeps the listed accounts as config.toml has them and adds the others picked', () => {
    const listed: AccountCfg[] = [
      { name: 'work', harness: 'claude', home: '~/.claude-work', command: 'cw', monthly: 200 },
      { name: 'gone', harness: 'codex', home: '~/.codex-gone', command: null },
    ];
    const view: SetupView = { accounts: [account('work', '~/.claude-work', { listed: true }), account('gone', '~/.codex-gone', { listed: true }), account('claude', '~/.claude', { command: 'claude' })], folders: [] };
    const after = accountsAfter(view, listed, ['~/.claude-work', '~/.claude']);
    expect(after.map((a) => a.name)).toEqual(['work', 'claude']);
    expect(after[0]).toBe(listed[0]);
    expect(after[1]).toMatchObject({ harness: 'claude', home: '~/.claude', command: 'claude', disabled: false });
  });

  it('writes the accounts, folders and apps picked into the settings at once', () => {
    const kept: AccountCfg[] = [{ name: 'claude', harness: 'claude', home: '~/.claude', command: null }];
    const emblems: EmblemsCfg = { kind: 'drawn', account: '', prompt: 'Pixel art.' };
    const next = configAfter(cfg([{ name: 'gone', harness: 'codex', home: '~/.codex', command: null }]), picked({ folders: ['~/code', '~/work'], terminal: 'iterm2', editor: 'zed', emblems }), kept, ['codex']);
    expect([next.accounts, next.scan, next.apps.terminal, next.apps.editor]).toEqual([kept, ['~/code', '~/work'], 'iterm2', 'zed']);
    expect(next.emblems).toEqual({ ...emblems, account: 'codex' });
  });

  it('says what each folder holds', () => {
    expect(folderNote(folder('~/projects', { repos: 30, worked: 14 }))).toBe('30 repos; your agents worked in 14 lately');
    expect(folderNote(folder('~/a', { repos: 2, worked: 2 }))).toBe('2 repos; your agents worked in both lately');
    expect(folderNote(folder('~/z', { repos: 5, worked: 5 }))).toBe('5 repos; your agents worked in all of them lately');
    expect(folderNote(folder('~/b', { repos: 1, worked: 1 }))).toBe('1 repo; your agents worked in it lately');
    expect(folderNote(folder('~/c', { repos: 1 }))).toBe('1 repo');
    expect(folderNote(folder('~/d', { exists: false }))).toBe('Not there yet');
    expect(folderNote(folder('~/e', { repos: null }))).toBe('Not looked in yet');
  });

  it('takes a typed folder as config.toml writes it', () => {
    expect(typedFolder('  ~/src/ ')).toBe('~/src');
    expect(typedFolder('   ')).toBeNull();
  });
});
