// Settings › Agents: puts the `codealong` command on PATH, the Codealong plugin in each Claude
// Code account and the agent skill in each other account's skills folder, so an agent can read
// the board and the todo list without anyone opening a terminal. The engine does the work
// (install.rs); this shows what's in place.

import { useEffect, useState } from 'react';
import { agentsStatus, installAgents } from '../api';
import { tildePath } from '../format';
import { HARNESS_WORD } from '../harness';
import type { AccountSkill, AgentsStatus, PluginState } from '../types';
import { Button, Icon } from './primitives';

/** The state word for an account's skill row, and whether it needs attention. */
export function skillState(a: AccountSkill): { label: string; tone: 'ok' | 'attn' | 'none' } {
  if (a.path === null) return { label: 'No skills folder known', tone: 'none' };
  if (a.installed && a.edited) return { label: a.current ? 'Edited' : 'Edited, update available', tone: 'none' };
  if (a.installed && a.current) return { label: 'Installed', tone: 'ok' };
  if (a.installed) return { label: 'Update available', tone: 'attn' };
  return { label: 'Not installed', tone: 'attn' };
}

/** What the install button on a skill row says. */
export function skillAction(a: AccountSkill): string {
  if (a.installed && a.edited) return a.current ? 'Restore' : 'Update anyway';
  return a.installed && a.current ? 'Install again' : a.installed ? 'Update' : 'Install';
}

/** The state word for a Claude Code account's plugin, against the version this build carries. */
export function pluginState(p: PluginState, shipped: string | null): { label: string; tone: 'ok' | 'attn' | 'none' } {
  if (!p.version) return { label: 'Not installed', tone: 'attn' };
  if (p.disabled) return { label: 'Turned off', tone: 'attn' };
  if (p.from) return { label: 'Installed from elsewhere', tone: 'none' };
  if (shipped && p.version !== shipped) return { label: 'Update available', tone: 'attn' };
  return { label: 'Installed', tone: 'ok' };
}

/** What the install button on a plugin row says. */
export function pluginAction(p: PluginState, shipped: string | null): string {
  if (!p.version) return 'Install plugin';
  if (p.disabled) return 'Turn on';
  if (p.from) return 'Use Codealong’s copy';
  return shipped && p.version !== shipped ? 'Update' : 'Install again';
}

/** A plugin version as Codealong stamps it (`0.2.0+1a2b3c4d`), for reading. */
export function versionText(v: string): string {
  const [base, stamp] = v.split('+');
  return stamp ? `${base} · ${stamp}` : base;
}

export function AgentsSection() {
  const [status, setStatus] = useState<AgentsStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  // What's being installed right now: 'command', an account's name, or 'all'.
  const [busy, setBusy] = useState<string | null>(null);

  const load = () =>
    agentsStatus()
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e: unknown) => setError(String(e)));

  useEffect(() => {
    void load();
  }, []);

  // A link or skill put in place from the terminal shows up on focus.
  useEffect(() => {
    const onFocus = () => void load();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  const install = async (what: string, command: boolean, accounts: string[]) => {
    setBusy(what);
    setError(null);
    try {
      setStatus(await installAgents(command, accounts));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const cmd = status?.command;
  const shipped = status?.pluginVersion ?? null;
  // Installing for all leaves edited copies, and a plugin installed from elsewhere, alone; each
  // has its own button.
  const installable = (status?.accounts ?? []).filter((a) => {
    if (a.plugin) return !!shipped && !a.plugin.from && pluginState(a.plugin, shipped).tone === 'attn';
    return a.path !== null && !(a.installed && a.current) && !(a.installed && a.edited);
  });
  const errors = [...(error ? [error.replace(/^Error: /, '')] : []), ...(status?.errors ?? [])];

  return (
    <>
      <div className="ca-settings-intro">
        <h3>Agents</h3>
        <p>
          Agents can use Codealong too. The <code className="ca-ic">codealong</code> command reads the board and changes todos, and the skill tells agents how to use it for todos, sessions and accounts.
          Claude Code gets both in the Codealong plugin, with hooks that let you answer its permission prompts and questions from Codealong. Codex, OpenCode, Antigravity and Cursor get the skill.
        </p>
      </div>
      <div className="ca-settings-cards">
        {status === null && !error && <p className="ca-empty">Checking what's installed…</p>}
        {cmd && (
          <div className="ca-ucard ca-acct">
            <div className="ca-ucard-head">
              <span className="ca-ucard-name">Command-line tool</span>
              <span className={['ca-tag', cmd.link ? (cmd.onPath ? 'is-ok' : 'is-attn') : 'is-attn'].join(' ')}>
                {cmd.link && cmd.onPath && <Icon name="check" size={12} stroke={2} />}
                {cmd.link ? (cmd.onPath ? 'Installed, on PATH' : 'Installed, not on PATH in your shell') : 'Not installed'}
              </span>
            </div>
            <dl className="ca-kv ca-agents-kv">
              <dt>Link</dt>
              <dd className="ca-code" title={cmd.link ?? undefined}>
                {cmd.link ? tildePath(cmd.link) : '—'}
              </dd>
              <dt>Runs</dt>
              <dd className="ca-code" title={cmd.target}>
                {tildePath(cmd.target)}
              </dd>
            </dl>
            {cmd.link && !cmd.onPath && <p className="ca-acct-note">Add {tildePath(cmd.link).replace(/\/codealong$/, '')} to PATH in your shell's profile, or run the command by its path.</p>}
            <div className="ca-acct-actions">
              <Button size="sm" icon="terminal" disabled={busy !== null} onClick={() => void install('command', true, [])}>
                {cmd.link ? 'Install again' : 'Install'}
              </Button>
              {!cmd.link && <span className="ca-acct-asof">A link in /usr/local/bin, or ~/.local/bin when that needs admin rights.</span>}
            </div>
          </div>
        )}
        {status && !status.skillFound && (
          <p className="ca-acct-note" role="alert">
            The skill folder isn't next to this build of Codealong, so it can't be installed from here.
          </p>
        )}
        {status && status.accounts.length === 0 && <p className="ca-empty">No accounts yet. Add one under Accounts and Codealong can go in its folder.</p>}
        {(status?.accounts ?? []).map((a) => {
          if (a.plugin) {
            const p = a.plugin;
            const state = pluginState(p, shipped);
            return (
              <div key={a.name} className="ca-ucard ca-acct ca-agents-row">
                <div className="ca-ucard-head">
                  <span className="ca-ucard-name">{a.name}</span>
                  <span className="ca-ucard-plan">{HARNESS_WORD[a.harness] ?? a.harness}</span>
                  <span className={['ca-tag', state.tone === 'ok' ? 'is-ok' : state.tone === 'attn' ? 'is-attn' : ''].join(' ')}>
                    {state.tone === 'ok' && <Icon name="check" size={12} stroke={2} />}
                    {state.label}
                  </span>
                </div>
                <dl className="ca-kv ca-agents-kv">
                  <dt>Plugin</dt>
                  <dd className="ca-code">{p.version ? `codealong ${versionText(p.version)}` : '—'}</dd>
                  {p.from && (
                    <>
                      <dt>From</dt>
                      <dd className="ca-code">{p.from}</dd>
                    </>
                  )}
                </dl>
                {p.from && <p className="ca-acct-note">It comes from another marketplace, so Codealong doesn’t update it. Use Codealong’s copy to install the one that comes with this app, which it keeps up to date.</p>}
                {p.disabled && <p className="ca-acct-note">It’s installed but not turned on, so its skill and hooks don’t run. Turn on switches it on.</p>}
                {a.installed && (
                  <p className="ca-acct-note">
                    {a.edited
                      ? `Your edited copy of the skill is still in ${tildePath(a.path ?? '')}, so sessions see it beside the plugin’s. Remove it to keep one.`
                      : `The skill Codealong put in ${tildePath(a.path ?? '')} before is still there. Installing the plugin takes it away, since the plugin has the skill.`}
                  </p>
                )}
                {shipped && (
                  <div className="ca-acct-actions">
                    <Button size="sm" disabled={busy !== null} onClick={() => void install(a.name, false, [a.name])}>
                      {busy === a.name ? 'Installing…' : pluginAction(p, shipped)}
                    </Button>
                  </div>
                )}
              </div>
            );
          }
          const state = skillState(a);
          const canInstall = a.path !== null && status?.skillFound;
          return (
            <div key={a.name} className="ca-ucard ca-acct ca-agents-row">
              <div className="ca-ucard-head">
                <span className="ca-ucard-name">{a.name}</span>
                <span className="ca-ucard-plan">{HARNESS_WORD[a.harness] ?? a.harness}</span>
                <span className={['ca-tag', state.tone === 'ok' ? 'is-ok' : state.tone === 'attn' ? 'is-attn' : ''].join(' ')}>
                  {state.tone === 'ok' && <Icon name="check" size={12} stroke={2} />}
                  {state.label}
                </span>
              </div>
              <dl className="ca-kv ca-agents-kv">
                <dt>Skills folder</dt>
                <dd className="ca-code" title={a.path ?? undefined}>
                  {a.path ? tildePath(a.path) : '—'}
                </dd>
              </dl>
              {a.path === null && <p className="ca-acct-note">Codealong doesn't know where {HARNESS_WORD[a.harness] ?? a.harness} reads personal skills, so there's nothing to install here yet.</p>}
              {a.installed && a.edited && (
                <p className="ca-acct-note">
                  This copy was changed after Codealong put it here, so updates leave it alone. {a.current ? 'Restore puts back' : 'Update anyway replaces it with'} the skill that came with this build.
                </p>
              )}
              {canInstall && (
                <div className="ca-acct-actions">
                  <Button size="sm" disabled={busy !== null} onClick={() => void install(a.name, false, [a.name])}>
                    {skillAction(a)}
                  </Button>
                </div>
              )}
            </div>
          );
        })}
        {status && installable.length > 1 && (
          <div className="ca-acct-add">
            <Button
              icon="check"
              disabled={busy !== null}
              onClick={() =>
                void install(
                  'all',
                  false,
                  installable.map((a) => a.name),
                )
              }
            >
              Install for all
            </Button>
          </div>
        )}
        {errors.map((e, i) => (
          <p key={i} className="ca-form-error" role="alert">
            {e}
          </p>
        ))}
        <p className="ca-acct-note ca-agents-foot">
          Codealong installs the plugin from a marketplace it keeps in its own folder, and updates it when Codealong updates. New Claude Code sessions get it; ones already running keep what they started with.
        </p>
      </div>
    </>
  );
}
