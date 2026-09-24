// Accounts: the Claude Code and Codex accounts Codealong reads. Saving rewrites the
// [[accounts]] tables in config.toml and leaves the rest of the file as you wrote it.

import { useEffect, useRef, useState } from 'react';
import { getAccounts, saveAccounts, signIn } from '../api';
import type { AccountCfg, AccountInfo, Harness, Usage } from '../types';
import { Button, Icon, Rich } from './primitives';
import { asOfPhrase, UsageMeter } from './UsageBar';

const HARNESS_WORD: Record<Harness, string> = { claude: 'Claude Code', codex: 'Codex' };

interface Draft {
  /** The account being edited, or null when adding one. */
  original: string | null;
  harness: Harness;
  name: string;
  home: string;
  command: string;
  /** Whether you typed in these fields; untouched ones follow the name. */
  touched: { home: boolean; command: boolean };
}

function defaultsFor(harness: Harness, name: string): { home: string; command: string } {
  const home = `~/.${name}`;
  const plain = name === harness;
  const command = plain ? harness : harness === 'claude' ? `CLAUDE_CONFIG_DIR=${home} claude` : `CODEX_HOME=${home} codex`;
  return { home, command };
}

/** The next free `claude2`, `claude3`… with its own folder. */
export function suggestAccount(harness: Harness, taken: AccountCfg[]): Omit<Draft, 'original' | 'touched'> {
  const names = new Set(taken.map((a) => a.name));
  const homes = new Set(taken.map((a) => a.home.replace(/\/$/, '')));
  let n = 1;
  const nameFor = (i: number) => (i === 1 ? harness : `${harness}${i}`);
  while (names.has(nameFor(n)) || homes.has(`~/.${nameFor(n)}`)) n++;
  const name = nameFor(n);
  return { harness, name, ...defaultsFor(harness, name) };
}

function toCfg(a: AccountCfg): AccountCfg {
  return { name: a.name, harness: a.harness, home: a.home, command: a.command };
}

interface AccountsDialogProps {
  /** The account to scroll to, from a click on its chip. */
  focus: string | null;
  usage: Usage[];
  terminal: string;
  now: number;
  onClose: () => void;
  onEditConfig: () => void;
}

export function AccountsDialog({ focus, usage, terminal, now, onClose, onEditConfig }: AccountsDialogProps) {
  const [accounts, setAccounts] = useState<AccountInfo[] | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);

  const load = () =>
    getAccounts()
      .then(setAccounts)
      .catch((e: unknown) => setError(String(e)));

  useEffect(() => {
    void load();
  }, []);

  const loaded = accounts !== null;
  useEffect(() => {
    if (loaded && focus) document.getElementById(`ca-acct-${focus}`)?.scrollIntoView({ block: 'nearest' });
  }, [loaded, focus]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  // Accounts added in another window, or signed in from the terminal, show up on focus.
  useEffect(() => {
    const onFocus = () => void load();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  const save = async (next: AccountCfg[]) => {
    setBusy(true);
    setError(null);
    try {
      await saveAccounts(next.map(toCfg));
      await load();
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const startAdd = (harness: Harness) => {
    setError(null);
    setRemoving(null);
    setDraft({ original: null, touched: { home: false, command: false }, ...suggestAccount(harness, accounts ?? []) });
  };

  const startEdit = (a: AccountInfo) => {
    setError(null);
    setRemoving(null);
    setDraft({ original: a.name, harness: a.harness, name: a.name, home: a.home, command: a.command ?? a.name, touched: { home: true, command: true } });
  };

  const submit = async () => {
    if (!draft || !accounts) return;
    const entry: AccountCfg = { name: draft.name.trim(), harness: draft.harness, home: draft.home.trim(), command: draft.command.trim() || null };
    const next = draft.original ? accounts.map((a) => (a.name === draft.original ? entry : a)) : [...accounts, entry];
    if (await save(next)) setDraft(null);
  };

  const remove = async (name: string) => {
    if (!accounts) return;
    if (await save(accounts.filter((a) => a.name !== name))) setRemoving(null);
  };

  const startSignIn = (name: string) => {
    signIn(name).catch((e: unknown) => setError(String(e)));
  };

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
        <div className="ca-segctl" role="radiogroup" aria-label="Harness">
          {(['claude', 'codex'] as Harness[]).map((h) => (
            <button
              key={h}
              type="button"
              role="radio"
              aria-checked={draft.harness === h}
              className={draft.harness === h ? 'is-on' : undefined}
              onClick={() => {
                if (draft.original) {
                  setDraft({ ...draft, harness: h });
                } else {
                  setDraft({ original: null, touched: { home: false, command: false }, ...suggestAccount(h, accounts ?? []) });
                }
              }}
            >
              {HARNESS_WORD[h]}
            </button>
          ))}
        </div>
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
        <small>
          {draft.harness === 'claude' ? (
            <>
              Where this account keeps its sign-in and sessions. Claude Code reads it from <code className="ca-ic">CLAUDE_CONFIG_DIR</code>.
            </>
          ) : (
            <>
              Where this account keeps its sign-in and sessions. Codex reads it from <code className="ca-ic">CODEX_HOME</code>.
            </>
          )}
        </small>
      </label>
      <label className="ca-field">
        <span>Command</span>
        <input value={draft.command} spellCheck={false} autoCapitalize="off" autoCorrect="off" onChange={(e) => setDraft({ ...draft, command: e.target.value, touched: { ...draft.touched, command: true } })} />
        <small>
          Starts this account in {terminal}. Codealong adds <code className="ca-ic">--resume</code> to reopen a session.
        </small>
      </label>
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
    <div
      className="ca-scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="ca-dialog" role="dialog" aria-modal="true" aria-labelledby="ca-accounts-title" ref={dialogRef}>
        <header className="ca-dialog-head">
          <h2 id="ca-accounts-title">Accounts</h2>
          <Button variant="ghost" size="sm" icon="x" aria-label="Close" onClick={onClose} />
        </header>
        <p className="ca-dialog-lede">Each subscription has its own config folder. Codealong reads sessions and usage from every folder listed here.</p>
        <div className="ca-dialog-body">
          {accounts === null && !error && <p className="ca-empty">Reading accounts…</p>}
          {(accounts ?? []).map((a) => {
            if (draft?.original === a.name) return <div key={a.name}>{form}</div>;
            const u = usage.find((x) => x.account === a.name);
            const status = a.signedIn ? (a.email ?? 'Signed in') : a.folderExists ? 'Not signed in' : 'Not set up';
            return (
              <div key={a.name} id={`ca-acct-${a.name}`} className={['ca-ucard', 'ca-acct', focus === a.name && 'is-focus'].filter(Boolean).join(' ')}>
                <div className="ca-ucard-head">
                  <span className="ca-ucard-name">{a.name}</span>
                  <span className="ca-ucard-plan">{[HARNESS_WORD[a.harness], u?.plan].filter(Boolean).join(' · ')}</span>
                  <span className={['ca-tag', a.signedIn ? 'is-ok' : 'is-attn'].join(' ')}>
                    {a.signedIn && <Icon name="check" size={12} stroke={2} />}
                    {status}
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
                  <dd className="ca-code">{a.command ?? a.name}</dd>
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
          {!draft && accounts !== null && (
            <div className="ca-acct-add">
              <Button icon="plus" onClick={() => startAdd('claude')}>
                Add Claude Code account
              </Button>
              <Button icon="plus" onClick={() => startAdd('codex')}>
                Add Codex account
              </Button>
            </div>
          )}
          {error && (
            <p className="ca-form-error" role="alert">
              {error.replace(/^Error: /, '')}
            </p>
          )}
        </div>
        <footer className="ca-dialog-foot">
          <button type="button" className="ca-linkbtn" onClick={onEditConfig}>
            Edit config.toml
          </button>
          <Button variant="primary" size="sm" onClick={onClose}>
            Done
          </Button>
        </footer>
      </div>
    </div>
  );
}
