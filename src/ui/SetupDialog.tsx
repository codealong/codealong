// Set up Codealong: a first run's wizard over the main window, and again from the app menu. Four
// steps confirm what a first run found and wrote to config.toml (src-tauri/src/setup.rs): the
// agent accounts to show, the folders to search for repos, the terminal and editor to open things
// in; the last offers to put Codealong in those accounts (the plugin, or the skill, and the
// command) and to open it at login. Nothing is saved until Done; Skip, Escape or the ✕ keep what
// the first run wrote. Either way it doesn't show by itself again (`finishSetup`).

import { useEffect, useState, type ReactNode } from 'react';
import { agentsStatus, finishSetup, getConfig, installAgents, saveConfig, setOpenAtLogin, setupView } from '../api';
import { HARNESS_WORD } from '../harness';
import { shortcutKeys } from '../quick';
import { errorText } from '../report';
import { accountsAfter, configAfter, firstPicks, folderNote, SETUP_STEPS, typedFolder, type SetupPicks, type SetupStep } from '../setup';
import type { ConfigView, SetupView } from '../types';
import { foundNote } from './AccountsSection';
import { Button, cx, Dialog, Icon, Kbd } from './primitives';
import { ProfileSelect } from './SettingsDialog';

const LEDE: Record<SetupStep, string> = {
  accounts: 'Codealong found these agent accounts on this Mac. It shows the sessions of the ones you keep, and how much of each plan they’ve used.',
  folders: 'Codealong looks for repos in these folders, three levels deep, and shows each one’s worktrees and the agents working in them.',
  apps: 'Where Codealong opens a session’s terminal, and a worktree’s folder.',
  ready: 'Codealong keeps these in config.toml, and Settings changes them whenever you like.',
};

const TITLE: Record<SetupStep, string> = { accounts: 'Your agents', folders: 'Your repos', apps: 'Your apps', ready: 'Ready' };

function Check({ checked, onChange, label, note }: { checked: boolean; onChange: (on: boolean) => void; label: ReactNode; note?: ReactNode }) {
  return (
    <label className="ca-setup-row">
      <span className="ca-check">
        <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
        <span className="ca-check-box" aria-hidden="true">
          <Icon name="check" size={11} stroke={2.5} />
        </span>
      </span>
      <span className="ca-setup-text">
        <span className="ca-setup-label">{label}</span>
        {note && <span className="ca-setup-note">{note}</span>}
      </span>
    </label>
  );
}

/** `list` with `item` in it or out of it, keeping the order it had. */
function toggled(list: string[], item: string, on: boolean): string[] {
  return on ? (list.includes(item) ? list : [...list, item]) : list.filter((x) => x !== item);
}

export function SetupDialog({ onClose, onDone, now }: { onClose: () => void; onDone: (said: string) => void; now: number }) {
  const [view, setView] = useState<SetupView | null>(null);
  const [cfg, setCfg] = useState<ConfigView | null>(null);
  const [picks, setPicks] = useState<SetupPicks | null>(null);
  const [step, setStep] = useState<SetupStep>('accounts');
  // Folders typed in, offered under the ones found.
  const [added, setAdded] = useState<string[]>([]);
  const [typed, setTyped] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // The settings once the view is in: it waits for a first run to write config.toml, and
    // settings read before would be the defaults, which Done would save over the first run's.
    setupView()
      .then((v) => getConfig().then((c) => [v, c] as const))
      .then(([v, c]) => {
        setView(v);
        setCfg(c);
        setPicks(firstPicks(v, c.config, c.terminals, c.editors));
      })
      .catch((e: unknown) => setError(errorText(e)));
  }, []);

  const at = SETUP_STEPS.indexOf(step);
  const last = at === SETUP_STEPS.length - 1;
  const pick = (change: Partial<SetupPicks>) => setPicks((p) => (p ? { ...p, ...change } : p));

  // Skipping keeps what the first run wrote; the wizard is still there in the app menu.
  const skip = () => {
    if (busy) return;
    // Not saying it failed: at worst the wizard shows again at the next launch.
    finishSetup().catch(() => {});
    onClose();
  };

  const done = async () => {
    if (!view || !cfg || !picks) return;
    setBusy(true);
    setError(null);
    try {
      const accounts = accountsAfter(view, cfg.config.accounts, picks.accounts);
      const next = configAfter(cfg.config, picks, accounts);
      if (JSON.stringify(next) !== JSON.stringify(cfg.config)) await saveConfig(next);
      const troubles: string[] = [];
      if (picks.agents && accounts.length > 0) {
        const status = await agentsStatus();
        const after = await installAgents(status.command.link === null, accounts.map((a) => a.name));
        troubles.push(...after.errors);
      }
      if (picks.atLogin) await setOpenAtLogin(true).catch((e: unknown) => troubles.push(errorText(e)));
      await finishSetup();
      onDone(troubles.length > 0 ? `Codealong is set up, but ${troubles.join(' ')}` : 'Codealong is set up.');
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const next = () => {
    if (last) void done();
    else setStep(SETUP_STEPS[at + 1]);
  };

  const addTyped = () => {
    const f = typedFolder(typed);
    if (!f || !picks) return;
    if (!view?.folders.some((x) => x.path === f) && !added.includes(f)) setAdded([...added, f]);
    pick({ folders: toggled(picks.folders, f, true) });
    setTyped('');
  };

  const quickKeys = shortcutKeys(cfg?.config.shortcuts?.quick_panel ?? cfg?.defaultShortcuts?.quick_panel);
  const kept = picks ? (view?.accounts ?? []).filter((a) => picks.accounts.includes(a.home)).length : 0;

  return (
    <Dialog
      title={view ? TITLE[step] : 'Set up Codealong'}
      lede={view ? LEDE[step] : undefined}
      busy={busy}
      onClose={skip}
      onSubmit={next}
      className="ca-setup"
      aside={
        <span className="ca-setup-steps" aria-label={`Step ${at + 1} of ${SETUP_STEPS.length}`}>
          {SETUP_STEPS.map((s, i) => (
            <span key={s} className={cx('ca-setup-dot', i === at && 'is-on', i < at && 'is-past')} />
          ))}
        </span>
      }
      foot={
        <>
          <Button variant="ghost" disabled={busy} onClick={skip}>
            Skip setup
          </Button>
          {at > 0 && (
            <Button disabled={busy} onClick={() => setStep(SETUP_STEPS[at - 1])}>
              Back
            </Button>
          )}
          <Button variant="primary" type="submit" disabled={busy || !picks}>
            {last ? 'Done' : 'Next'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {error && <p className="ca-setup-error">{error}</p>}
        {!view || !picks || !cfg ? (
          !error && <p className="ca-setup-wait">Looking for your agents and repos…</p>
        ) : step === 'accounts' ? (
          view.accounts.length === 0 ? (
            <p className="ca-setup-empty">No agent accounts on this Mac yet. Codealong reads Claude Code, Codex, OpenCode, Antigravity and Cursor: sign in to one, and Settings › Accounts finds it.</p>
          ) : (
            <div className="ca-setup-list">
              {view.accounts.map((a) => (
                <Check
                  key={a.home}
                  checked={picks.accounts.includes(a.home)}
                  onChange={(on) => pick({ accounts: toggled(picks.accounts, a.home, on) })}
                  label={
                    <>
                      <span className="ca-setup-name">{a.name}</span>
                      <span className="ca-setup-kind">{HARNESS_WORD[a.harness]}</span>
                      <span className="ca-code ca-setup-path">{a.home}</span>
                    </>
                  }
                  note={foundNote(a, now)}
                />
              ))}
            </div>
          )
        ) : step === 'folders' ? (
          <>
            <div className="ca-setup-list">
              {[...view.folders, ...added.map((path) => ({ path, exists: true, repos: null, worked: 0, scanned: false }))].map((f) => (
                <Check
                  key={f.path}
                  checked={picks.folders.includes(f.path)}
                  onChange={(on) => pick({ folders: toggled(picks.folders, f.path, on) })}
                  label={<span className="ca-code">{f.path}</span>}
                  note={added.includes(f.path) ? 'Added here' : folderNote(f)}
                />
              ))}
            </div>
            <div className="ca-field ca-setup-add">
              <input
                value={typed}
                placeholder="Another folder, such as ~/src"
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                onChange={(e) => setTyped(e.target.value)}
                onKeyDown={(e) => {
                  // Return adds the folder here rather than going on to the next step.
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    addTyped();
                  }
                }}
              />
              <Button icon="plus" disabled={typedFolder(typed) === null} onClick={addTyped}>
                Add folder
              </Button>
            </div>
          </>
        ) : step === 'apps' ? (
          <div className="ca-setup-fields">
            <label className="ca-field">
              <span>Terminal</span>
              <ProfileSelect value={picks.terminal} profiles={cfg.terminals} onChange={(terminal) => pick({ terminal })} />
              <small>Where sessions open, and new ones start.</small>
            </label>
            <label className="ca-field">
              <span>Editor</span>
              <ProfileSelect value={picks.editor} profiles={cfg.editors} onChange={(editor) => pick({ editor })} />
              <small>Opens a worktree’s folder. The Open in menus offer the other editors installed here.</small>
            </label>
          </div>
        ) : (
          <>
            <div className="ca-setup-list">
              <Check
                checked={picks.agents && kept > 0}
                onChange={(agents) => pick({ agents })}
                label="Let your agents use Codealong"
                note={
                  kept > 0
                    ? `Puts the Codealong plugin in Claude Code and the skill in the other agents (${kept === 1 ? 'the account' : kept === 2 ? 'both accounts' : `all ${kept} accounts`} you kept), and the codealong command on your PATH: agents can then see the board and your todos, and pick an account with room.`
                    : 'Once you keep an account, Settings › Agents puts Codealong in it.'
                }
              />
              <Check checked={picks.atLogin} onChange={(atLogin) => pick({ atLogin })} label="Open Codealong when you log in" note="So it’s keeping track of your agents from the start. Settings › General turns it off again." />
            </div>
            {quickKeys && (
              <p className="ca-setup-tip">
                <Kbd keys={quickKeys} /> in any app opens the quick panel: find a session, answer one that waits on you, or start one.
              </p>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}
