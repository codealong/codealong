// Settings › Accounts: the Claude Code, Codex, OpenCode and Antigravity accounts Codealong
// reads, and under Found on this Mac the ones it could (detect.rs). Saving rewrites the
// [[accounts]] tables in config.toml and leaves the rest of the file as you wrote it. Adding an
// account can put Codealong in it too (install.rs: the plugin in Claude Code, the skill elsewhere), as
// Settings › Agents does; that page is where each account's plugin or skill shows.

import { useEffect, useState } from 'react';
import { agentsStatus, foundAccounts, getAccounts, installAgents, saveAccounts, setPreferred, signIn } from '../api';
import { agoPhrase } from '../format';
import { HARNESSES, HARNESS_WORD } from '../harness';
import type { AccountCfg, AccountInfo, FoundAccount, Harness, StartPlace, Usage } from '../types';
import { Button, cx, Icon, Rich, Seg } from './primitives';
import { asOfPhrase, UsageMeter } from './UsageBar';

interface Draft {
  /** The account being edited, or null when adding one. */
  original: string | null;
  harness: Harness;
  name: string;
  home: string;
  command: string;
  /** Where its sessions start unless you pick another; null goes where the last one did. */
  start: StartPlace | null;
  /** Whether you typed in these fields; untouched ones follow the name. */
  touched: { home: boolean; command: boolean };
}

/** Where a harness keeps a second account's files, and the command that runs it there. */
function defaultsFor(harness: Harness, name: string): { home: string; command: string } {
  const plain = name === harness;
  switch (harness) {
    case 'claude':
      return { home: `~/.${name}`, command: plain ? 'claude' : `CLAUDE_CONFIG_DIR=~/.${name} claude` };
    case 'codex':
      return { home: `~/.${name}`, command: plain ? 'codex' : `CODEX_HOME=~/.${name} codex` };
    case 'opencode':
      // OpenCode's data folder is `<XDG_DATA_HOME>/opencode`.
      return plain ? { home: '~/.local/share/opencode', command: 'opencode' } : { home: `~/.${name}/opencode`, command: `XDG_DATA_HOME=~/.${name} opencode` };
    case 'antigravity':
      // Antigravity keeps every app data folder under ~/.gemini; its command line is `agy`
      // whatever the account is called, so the account takes no command of its own.
      return { home: `~/.gemini/${name}`, command: '' };
  }
}

/** Where an account can start a session from Codealong, as start.rs's `places_of` has it: the
 *  Claude and Codex apps each run only the account in the default folder, Codealong runs any
 *  Claude Code, Codex or OpenCode account over its ACP adapter, and Antigravity starts in the
 *  terminal once its command line is installed (the engine says when it isn't). Every account
 *  that starts in the terminal can start in the background: Claude Code's own service, or the
 *  terminal behind your windows. */
export function startPlaces(harness: Harness, home: string): StartPlace[] {
  const main = (dir: string) => new RegExp(`^(~|/Users/[^/]+)/\\.${dir}/?$`).test(home.trim());
  switch (harness) {
    case 'claude':
      return main('claude') ? ['terminal', 'background', 'claude-app', 'codealong'] : ['terminal', 'background', 'codealong'];
    case 'codex':
      return main('codex') ? ['codex-app', 'terminal', 'background', 'codealong'] : ['terminal', 'background', 'codealong'];
    case 'opencode':
      return ['terminal', 'background', 'codealong'];
    case 'antigravity':
      return ['terminal', 'background'];
  }
}

export function placeName(place: StartPlace, terminal: string): string {
  return place === 'terminal' ? terminal : place === 'background' ? 'Background' : place === 'claude-app' ? 'Claude app' : place === 'codex-app' ? 'Codex app' : 'Codealong';
}

/** What the config folder holds for a harness, and how it's told about a second one. */
function folderHelp(harness: Harness): React.ReactNode {
  switch (harness) {
    case 'claude':
      return (
        <>
          Where this account keeps its sign-in and sessions. Claude Code reads it from <code className="ca-ic">CLAUDE_CONFIG_DIR</code>.
        </>
      );
    case 'codex':
      return (
        <>
          Where this account keeps its sign-in and sessions. Codex reads it from <code className="ca-ic">CODEX_HOME</code>.
        </>
      );
    case 'opencode':
      return (
        <>
          OpenCode's data folder, with its sessions database. It's <code className="ca-ic">opencode</code> under <code className="ca-ic">XDG_DATA_HOME</code>.
        </>
      );
    case 'antigravity':
      return <>Antigravity's app data folder, with its conversations. The app's own is ~/.gemini/antigravity; its command line's, ~/.gemini/antigravity-cli beside it, is read with it.</>;
  }
}

/** What Codealong does with the command for a harness. */
function commandHelp(harness: Harness, terminal: string): string {
  switch (harness) {
    case 'claude':
      return `Starts this account in ${terminal}. Codealong adds --resume to reopen a session.`;
    case 'codex':
      return `Starts this account in ${terminal} and signs it in there. Codealong adds resume to reopen a thread.`;
    case 'opencode':
      return `Starts this account in ${terminal}. Codealong adds --session to reopen a session.`;
    case 'antigravity':
      return `Its command line, agy unless you give another, starts and resumes conversations in ${terminal}. The app's own conversations open in the app.`;
  }
}

/** The next free `claude2`, `claude3`… with its own folder. */
export function suggestAccount(harness: Harness, taken: AccountCfg[]): Omit<Draft, 'original' | 'touched'> {
  const names = new Set(taken.map((a) => a.name));
  const homes = new Set(taken.map((a) => a.home.replace(/\/$/, '')));
  let n = 1;
  const nameFor = (i: number) => (i === 1 ? harness : `${harness}${i}`);
  while (names.has(nameFor(n)) || homes.has(defaultsFor(harness, nameFor(n)).home)) n++;
  const name = nameFor(n);
  return { harness, name, start: null, ...defaultsFor(harness, name) };
}

/** Where a found account stands, in words: its sign-in, when it was last used, and the
 * wrapper or alias that runs it. */
export function foundNote(f: FoundAccount, now: number): string {
  const who = f.signedIn ? (f.email ? `Signed in as ${f.email}` : 'Signed in') : null;
  const used = f.lastUsed !== null ? `used ${agoPhrase(f.lastUsed, now)}` : null;
  let where: string;
  if (f.state === 'active') where = [who ?? 'Not signed in', used].filter(Boolean).join(', ');
  else if (f.state === 'signedIn') where = `${who ?? 'Signed in'}, ${used ? `last ${used}` : 'no sessions yet'}`;
  else where = f.folderExists ? 'Not signed in' : 'Installed, not set up yet';
  const runs = f.via && f.command ? `starts with ${f.command} from ${f.via}` : null;
  return [where, runs].filter(Boolean).join(' · ');
}

function toCfg(a: AccountCfg): AccountCfg {
  return { name: a.name, harness: a.harness, home: a.home, command: a.command, start: a.start ?? null, preferred: a.preferred ?? false };
}

interface AccountsSectionProps {
  /** The account to scroll to, from a click on its chip. */
  focus: string | null;
  usage: Usage[];
  terminal: string;
  now: number;
  /** Something was saved: the settings behind the other sections changed too. */
  onSaved: () => void;
}

export function AccountsSection({ focus, usage, terminal, now, onSaved }: AccountsSectionProps) {
  const [accounts, setAccounts] = useState<AccountInfo[] | null>(null);
  const [found, setFound] = useState<FoundAccount[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Whether an account you add gets Codealong (the plugin, or the skill), so its agents can use
  // the board. Settings › Agents shows where each account stands.
  const [withSkill, setWithSkill] = useState(true);

  const load = () => {
    // Looking for accounts reads a few folders and asks the keychain; it can come after the list.
    foundAccounts()
      .then(setFound)
      .catch(() => setFound([]));
    return getAccounts()
      .then(setAccounts)
      .catch((e: unknown) => setError(String(e)));
  };

  useEffect(() => {
    void load();
  }, []);

  const loaded = accounts !== null;
  useEffect(() => {
    if (loaded && focus) document.getElementById(`ca-acct-${focus}`)?.scrollIntoView({ block: 'nearest' });
  }, [loaded, focus]);

  // Accounts added in another window, or signed in from the terminal, show up on focus.
  useEffect(() => {
    const onFocus = () => void load();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  // The star goes through the engine, which publishes it at once, so the quick panel's Auto
  // goes by it straight away; it shows here before that.
  const star = async (name: string, on: boolean) => {
    setError(null);
    setAccounts((list) => list && list.map((x) => (x.name === name ? { ...x, preferred: on } : x)));
    try {
      await setPreferred(name, on);
    } catch (e) {
      setError(String(e));
      await load();
    }
  };

  const save = async (next: AccountCfg[]) => {
    setBusy(true);
    setError(null);
    try {
      await saveAccounts(next.map(toCfg));
      await load();
      onSaved();
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  /** Puts Codealong in a new account (the plugin in Claude Code, the skill elsewhere), and the
   *  command on PATH if it isn't yet. */
  const installSkill = async (name: string) => {
    if (!withSkill) return;
    try {
      const status = await agentsStatus();
      const after = await installAgents(status.command.link === null, [name]);
      if (after.errors.length > 0) setError(after.errors.join(' '));
    } catch (e) {
      setError(String(e));
    }
  };

  const addFound = async (f: FoundAccount) => {
    if (!accounts) return;
    setRemoving(null);
    if (await save([...accounts, toCfg(f)])) await installSkill(f.name);
  };

  const startAdd = (harness: Harness) => {
    setError(null);
    setRemoving(null);
    setDraft({ original: null, touched: { home: false, command: false }, ...suggestAccount(harness, accounts ?? []) });
  };

  const startEdit = (a: AccountInfo) => {
    setError(null);
    setRemoving(null);
    setDraft({ original: a.name, harness: a.harness, name: a.name, home: a.home, command: a.command ?? a.name, start: a.start ?? null, touched: { home: true, command: true } });
  };

  const submit = async () => {
    if (!draft || !accounts) return;
    // A place the folder or harness no longer offers falls back to the usual one.
    const start = draft.start && startPlaces(draft.harness, draft.home).includes(draft.start) ? draft.start : null;
    const preferred = accounts.find((a) => a.name === draft.original)?.preferred ?? false;
    const entry: AccountCfg = { name: draft.name.trim(), harness: draft.harness, home: draft.home.trim(), command: draft.command.trim() || null, start, preferred };
    const next = draft.original ? accounts.map((a) => (a.name === draft.original ? entry : a)) : [...accounts, entry];
    if (await save(next)) {
      setDraft(null);
      if (!draft.original) await installSkill(entry.name);
    }
  };

  const skillBox = (label: string) => (
    <label className="ca-check ca-acct-skill" title="Agents on the account can then use the codealong command: the board, todos, and which account has room.">
      <input type="checkbox" checked={withSkill} onChange={(e) => setWithSkill(e.target.checked)} />
      <span className="ca-check-box" aria-hidden="true">
        <Icon name="check" size={11} stroke={2.5} />
      </span>
      {label}
    </label>
  );

  const remove = async (name: string) => {
    if (!accounts) return;
    if (await save(accounts.filter((a) => a.name !== name))) setRemoving(null);
  };

  const startSignIn = (name: string) => {
    signIn(name).catch((e: unknown) => setError(String(e)));
  };

  const draftPlaces = draft ? startPlaces(draft.harness, draft.home) : [];
  const draftStart = draft?.start && draftPlaces.includes(draft.start) ? draft.start : null;

  const form = draft && (
    <form
      className="ca-ucard ca-acct-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <div className="ca-acct-form-head">
        <span className="ca-ucard-name">{draft.original ? `Edit ${draft.original}` : 'New account'}</span>
        <Seg
          label="Harness"
          options={HARNESSES.map((h) => ({ id: h, label: HARNESS_WORD[h] }))}
          value={draft.harness}
          onPick={(h) => {
            if (draft.original) {
              setDraft({ ...draft, harness: h });
            } else {
              setDraft({ original: null, touched: { home: false, command: false }, ...suggestAccount(h, accounts ?? []) });
            }
          }}
        />
      </div>
      <label className="ca-field">
        <span>Name</span>
        <input
          value={draft.name}
          autoFocus
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => {
            const name = e.target.value;
            const d = defaultsFor(draft.harness, name.trim() || draft.harness);
            setDraft({ ...draft, name, home: draft.touched.home ? draft.home : d.home, command: draft.touched.command ? draft.command : d.command });
          }}
        />
        <small>What the board shows on this account's sessions.</small>
      </label>
      <label className="ca-field">
        <span>Config folder</span>
        <input value={draft.home} spellCheck={false} autoCapitalize="off" autoCorrect="off" onChange={(e) => setDraft({ ...draft, home: e.target.value, touched: { ...draft.touched, home: true } })} />
        <small>{folderHelp(draft.harness)}</small>
      </label>
      <label className="ca-field">
        <span>Command</span>
        <input value={draft.command} spellCheck={false} autoCapitalize="off" autoCorrect="off" onChange={(e) => setDraft({ ...draft, command: e.target.value, touched: { ...draft.touched, command: true } })} />
        <small>{commandHelp(draft.harness, terminal)}</small>
      </label>
      {draftPlaces.length > 1 && (
        <div className="ca-field">
          <span id="ca-acct-start">New sessions start in</span>
          <Seg labelledBy="ca-acct-start" options={[null, ...draftPlaces].map((p) => ({ id: p, label: p ? placeName(p, terminal) : 'Last used' }))} value={draftStart} onPick={(p) => setDraft({ ...draft, start: p })} />
          <small>Where the quick panel and codealong start open a new session on this account. You can still pick another place there.</small>
        </div>
      )}
      {!draft.original && skillBox(draft.harness === 'claude' ? 'Install the Codealong plugin' : 'Install the Codealong skill')}
      <div className="ca-acct-actions">
        <Button variant="primary" size="sm" type="submit" disabled={busy}>
          {draft.original ? 'Save' : 'Add account'}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => setDraft(null)}>
          Cancel
        </Button>
      </div>
    </form>
  );

  return (
    <>
      <div className="ca-settings-intro">
        <h3>Accounts</h3>
        <p>Each subscription has its own config folder. Codealong reads sessions and usage from every folder listed here.</p>
      </div>
      <div className="ca-settings-cards">
      {accounts === null && !error && <p className="ca-empty">Reading accounts…</p>}
      {(accounts ?? []).map((a) => {
        if (draft?.original === a.name) return <div key={a.name}>{form}</div>;
        const u = usage.find((x) => x.account === a.name);
        const status = a.signedIn ? (a.email ?? 'Signed in') : a.folderExists ? 'Not signed in' : 'Not set up';
        return (
          <div key={a.name} id={`ca-acct-${a.name}`} className={cx('ca-ucard', 'ca-acct', focus === a.name && 'is-focus')}>
            <div className="ca-ucard-head">
              <button
                type="button"
                className={cx('ca-acct-star', a.preferred ? 'is-on' : '')}
                aria-pressed={!!a.preferred}
                aria-label={`Preferred: ${a.name}`}
                title={a.preferred ? 'Preferred: Auto starts this account while it has room. Click to stop preferring it.' : 'Prefer this account: Auto starts preferred accounts while one has room, and others only when they’re all near a limit.'}
                disabled={busy}
                onClick={() => void star(a.name, !a.preferred)}
              >
                <Icon name="star" size={16} />
              </button>
              <span className="ca-ucard-name">{a.name}</span>
              <span className="ca-ucard-plan">{[HARNESS_WORD[a.harness], u?.plan].filter(Boolean).join(' · ')}</span>
              <span className="ca-acct-tags">
                <span className={['ca-tag', a.signedIn ? 'is-ok' : 'is-attn'].join(' ')}>
                  {a.signedIn && <Icon name="check" size={12} stroke={2} />}
                  {status}
                </span>
              </span>
            </div>
            {u && u.meters.length > 0 && (
              <div className="ca-acct-meters">
                {u.meters.map((m) => (
                  <UsageMeter key={m.key} meter={m} now={now} compact={false} />
                ))}
              </div>
            )}
            {u?.error && !u.needsSignIn && (
              <p className="ca-acct-note">
                <Rich text={u.error} now={now} />
              </p>
            )}
            <dl className="ca-kv">
              <dt>Folder</dt>
              <dd className="ca-code">{a.home}</dd>
              <dt>Command</dt>
              <dd className="ca-code">{a.command ?? (a.harness === 'antigravity' ? 'agy' : a.name)}</dd>
              {startPlaces(a.harness, a.home).length > 1 && (
                <>
                  <dt>Starts in</dt>
                  <dd>{a.start ? placeName(a.start, terminal) : 'Last used'}</dd>
                </>
              )}
            </dl>
            {removing === a.name ? (
              <div className="ca-acct-confirm" role="alert">
                <span>
                  Stop reading <strong>{a.name}</strong>? Its sessions leave the board. Nothing is deleted from disk.
                </span>
                <Button size="sm" disabled={busy} onClick={() => void remove(a.name)}>
                  Remove
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setRemoving(null)}>
                  Cancel
                </Button>
              </div>
            ) : (
              <div className="ca-acct-actions">
                {!a.signedIn && (
                  <Button size="sm" icon="terminal" onClick={() => startSignIn(a.name)}>
                    Sign in with {terminal}
                  </Button>
                )}
                <Button variant="ghost" size="sm" onClick={() => startEdit(a)}>
                  Edit
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setRemoving(a.name)}>
                  Remove
                </Button>
                {u && asOfPhrase(u, now) && <span className="ca-acct-asof">{asOfPhrase(u, now)}</span>}
              </div>
            )}
          </div>
        );
      })}
      {draft && !draft.original && form}
      {!draft && accounts !== null && found.length > 0 && (
        <section className="ca-found" aria-labelledby="ca-found-title">
          <h4 className="ca-label" id="ca-found-title">
            Found on this Mac
          </h4>
          <div className="ca-found-list">
            {found.map((f) => (
              <div key={f.home} className="ca-found-row">
                <div className="ca-found-main">
                  <div className="ca-found-title">
                    <span className="ca-ucard-name">{f.name}</span>
                    <span className="ca-ucard-plan">{HARNESS_WORD[f.harness]}</span>
                    <span className="ca-code ca-found-home">{f.home}</span>
                  </div>
                  <p className="ca-found-note">{foundNote(f, now)}</p>
                </div>
                <Button size="sm" icon="plus" disabled={busy} onClick={() => void addFound(f)}>
                  Add
                </Button>
              </div>
            ))}
          </div>
          {skillBox('Install the Codealong skill in accounts you add')}
        </section>
      )}
      {!draft && accounts !== null && (
        <div className="ca-acct-add">
          {HARNESSES.map((h) => (
            <Button key={h} icon="plus" onClick={() => startAdd(h)}>
              Add {HARNESS_WORD[h]} account
            </Button>
          ))}
        </div>
      )}
      {error && (
        <p className="ca-form-error" role="alert">
          {error.replace(/^Error: /, '')}
        </p>
      )}
      </div>
    </>
  );
}
