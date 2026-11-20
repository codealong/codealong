// Settings › Decisions (decisions.rs, decisions.ts): the decision models, Jev's always among
// them, each with what reads its way and what it falls back to; each provider's key, kept in the
// keychain (`KeyField`, which Settings › Appearance's Session emblems shows too); what the models'
// requests used (JevUsage.tsx); and a switch per feature saying what it does and sends.

import { useEffect, useState, useSyncExternalStore } from 'react';
import { decisionKeys, openUrl, setDecisionKey, testDecider } from '../api';
import { choices, decisionsOf, forGroup, modelsOf, newModelProblem, provider, PROVIDERS, providersInUse, type Provider, withModels, withoutModel } from '../decisions';
import { keysIn } from '../platform';
import { errorText } from '../report';
import type { Config, DecisionModelCfg, DecisionsCfg } from '../types';
import { JevUsage } from './JevUsage';
import { Button, CheckBox, cx, Icon, Seg } from './primitives';
import { ProfileSelect } from './ProfileSelect';

type DecisionFeature = 'quick_panel' | 'sessions' | 'todos' | 'pull_requests';

/** Each switch: what it does, and what it sends to the decision model that may read it. */
const DECISION_FEATURES: { id: DecisionFeature; label: string; does: string; sends: string }[] = [
  {
    id: 'quick_panel',
    label: 'Quick panel suggestions',
    does: 'As you type, suggests the session to send it to or the worktree to start it in; ⏎ does the first. Warns when a running session may already be on the same work, puts the sessions it thinks you mean first in the search, starts work that needs a service (your mail, say) on an account whose MCP servers reach it, and, when Auto starts Claude Code, a smaller model for a small change.',
    sends: 'What you type, with the images you paste into it, and the project’s session titles and latest lines, branches, pull request titles and worktree names, with the names of the services your accounts’ MCP servers reach and of their tools when the accounts differ. Once each time you pause typing. A session whose project or boundary group chose another model goes by its title and where it works alone.',
  },
  {
    id: 'sessions',
    label: 'Session status',
    does: 'Reads how each turn ended. A reply that asks you something shows as Needs you. A session quiet for a long time is marked stuck only when its command waits for input, not when it’s a long build. A command left running in the background says whether it’s polling for something (a review, CI), running a job, a server left running, or waiting on you. Shows how far the work got (committed, pushed, landed), which worktrees look finished, and how risky a command waiting for your approval looks.',
    sends: 'The end of each session’s last reply, its title and first prompt, and the command it’s running or waiting on. Once per turn.',
  },
  {
    id: 'todos',
    label: 'Todos',
    does: 'Links a new session to the todo it’s for when that’s clear. Says when a todo’s linked work looks done, with Mark done and Not yet. Suggests a todo’s project, labels and the todos it waits on as chips you click; nothing changes until you do.',
    sends: 'Your open todos’ titles and descriptions, the images in a todo you’re writing, your label and project names, new sessions’ titles and first prompts, and linked sessions’ latest replies with their pull requests’ titles and states.',
  },
  {
    id: 'pull_requests',
    label: 'Pull request reviews',
    does: 'Lists a pull request’s comments in its panel and marks the ones that ask the author for a change or an answer, so you can see what still needs a reply.',
    sends: 'The pull request’s title and others’ comments on threads still open, when you open it in Codealong. Answers are kept, so only new or edited comments are sent again.',
  },
];

/** A feature's name in the usage: its switch's label, Session emblems' (Settings › Appearance), or
 *  the Test button's. */
function decisionFeatureName(feature: string): string {
  return DECISION_FEATURES.find((f) => f.id === feature)?.label ?? ({ emblems: 'Session emblems', test: 'Test button', prompts: 'What your prompts did' } as Record<string, string>)[feature] ?? feature;
}

/** Which providers the keychain has a key for, as it was last read: null until it has been. One
 *  value for every field that shows a key, so a key saved or removed in one shows in the others
 *  (Settings' search shows Appearance's Session emblems and Settings › Decisions at once). */
let keysSaved: Record<string, boolean> | null = null;
const keyWatchers = new Set<() => void>();

function watchKeys(tell: () => void): () => void {
  keyWatchers.add(tell);
  return () => {
    keyWatchers.delete(tell);
  };
}

/** Reads the keychain again, for every field that shows a key. One that can't be read counts as
 *  none: the box to paste one into shows, and saving a key there says what's wrong. */
async function readKeys(): Promise<void> {
  const has = await decisionKeys().catch(() => ({}) as Record<string, boolean>);
  if (keysSaved && JSON.stringify(has) === JSON.stringify(keysSaved)) return;
  keysSaved = has;
  keyWatchers.forEach((tell) => tell());
}

/** `keysSaved`, read again as a field that shows them opens. */
export function useDecisionKeys(): Record<string, boolean> | null {
  const has = useSyncExternalStore(watchKeys, () => keysSaved);
  useEffect(() => {
    void readKeys();
  }, []);
  return has;
}

/**
 * A provider's key field: a box to paste one into while there's none, else Test (on `model`),
 * Replace and Remove. The key goes to the keychain as its button is pressed and never comes back
 * to the page; `onChanged` hears that a button was pressed, once the keychain is read again.
 * Settings › Decisions', and Session emblems' while emoji want one (`EmblemsFields`), which the
 * setup wizard shows too: it's no form of its own, so Return saves the key, not the form it's in.
 */
export function KeyField({ provider: p, label, model, saved, onChanged }: { provider: Provider; label: string; model: string; saved: string; onChanged?: () => void }) {
  const keys = useDecisionKeys();
  const hasKey = keys == null ? null : !!keys[p.id];
  const [editing, setEditing] = useState(false);
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  const run = async (what: () => Promise<string | null>) => {
    setBusy(true);
    setNote(null);
    try {
      const said = await what();
      if (said) setNote({ ok: true, text: said });
    } catch (e) {
      setNote({ ok: false, text: errorText(e) });
    }
    await readKeys();
    onChanged?.();
    setBusy(false);
  };

  const saveKey = () => {
    if (busy || !key.trim()) return;
    void run(async () => {
      await setDecisionKey(p.id, key);
      setKey('');
      setEditing(false);
      return saved;
    });
  };

  return (
    <div className="ca-field">
      <span>{label}</span>
      {editing || hasKey === false ? (
        <div className="ca-shortcut-row">
          <input
            type="password"
            className="ca-jev-key"
            value={key}
            placeholder={`Paste your ${p.word} API key`}
            autoComplete="off"
            spellCheck={false}
            aria-label={`${p.word} API key`}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                saveKey();
              }
            }}
          />
          <Button size="sm" disabled={busy || !key.trim()} onClick={saveKey}>
            Save key
          </Button>
          {editing && (
            <Button variant="ghost" size="sm" onClick={() => setEditing(false)}>
              Cancel
            </Button>
          )}
        </div>
      ) : hasKey ? (
        <div className="ca-shortcut-row">
          <span className="ca-jev-saved">
            <Icon name="lock" size={12} />
            Saved in the keychain
          </span>
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => void run(() => testDecider(p.id, model))}>
            Test
          </Button>
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => setEditing(true)}>
            Replace
          </Button>
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => void run(async () => (await setDecisionKey(p.id, ''), null))}>
            Remove
          </Button>
        </div>
      ) : (
        <p className="ca-empty">Reading the keychain…</p>
      )}
      {note && <p className={note.ok ? 'ca-jev-ok' : 'ca-form-error'}>{note.text}</p>}
      <small>
        Kept in your login keychain, never in config.toml. {p.word} charges {p.price}, nothing for what it answers.{' '}
        <button type="button" className="ca-linkbtn" onClick={() => void openUrl(p.keys)}>
          Get a key from {p.word}
        </button>
      </small>
    </div>
  );
}

/** A model being added in Settings › Decisions. */
interface ModelDraft {
  provider: string;
  name: string;
  model: string;
}

function modelDraft(p: Provider, taken: string[]): ModelDraft {
  const name = taken.includes(p.name) ? '' : p.name;
  return { provider: p.id, name, model: p.model };
}

/**
 * Settings › Decisions: the decision models, Jev's always among them, each with what reads its
 * way (the default, projects, boundary groups) and what it falls back to; each provider's key
 * (`KeyField`); what the models' requests have used and cost; and a switch per feature that says
 * what it does and what it sends. Which model reads a project's sessions is set on the project
 * (Settings › Projects), a boundary group's on the group (Settings › Routing).
 */
export function DecisionsPane({ cfg, disabled, onSave }: { cfg: Config; disabled: boolean; onSave: (next: Config) => void }) {
  const d = decisionsOf(cfg);
  const models = modelsOf(d);
  const keys = useDecisionKeys();
  // Bumped after each of the keys' buttons, so the usage reads again with the Test button's
  // request in it.
  const [asked, setAsked] = useState(0);
  const [adding, setAdding] = useState<ModelDraft | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const save = (next: Partial<DecisionsCfg>) => onSave({ ...cfg, decisions: { ...d, ...next } });
  const change = (name: string, patch: Partial<DecisionModelCfg>) => save({ models: withModels(models.map((m) => (m.name === name ? { ...m, ...patch } : m))) });
  // The features want a key for some model they may ask.
  const anyKey = keys == null ? null : models.some((m) => keys[m.provider]);
  const noKey = anyKey === false;
  const groups = (cfg.routing?.groups ?? []).filter((g) => g.boundary);
  const problem = adding && newModelProblem(d, adding);

  return (
    <>
      <div className="ca-settings-intro">
        <h3>Decisions</h3>
        <p>
          A decision model answers narrow questions in a fraction of a second: which session a prompt is for, whether a reply asks you something, which todo a session is for. TypeSafe’s Jev is the first; OpenAI’s and Google’s are others. Which one reads a project is where its sessions’ words go, so you choose it per project (Settings › Projects) and per boundary group (Settings › Routing); the default reads the rest. Each feature is off until you turn it on, and each says what it sends.
        </p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        <div className="ca-field">
          <span>Models</span>
          <div className="ca-settings-cards">
            {models.map((m) => {
              const p = provider(m.provider);
              const projects = cfg.projects.filter((x) => x.decisions === m.name).map((x) => x.name);
              const bounded = groups.filter((g) => forGroup(g) === m.name).map((g) => g.name);
              return (
                <div key={m.name} className="ca-ucard">
                  <div className="ca-ucard-head">
                    <span className="ca-ucard-name">{m.name}</span>
                    <span className="ca-ucard-plan">
                      {p?.word ?? m.provider} · {m.model}
                    </span>
                    {d.default === m.name && <span className="ca-tag is-ok">Default</span>}
                    {keys && !keys[m.provider] && <span className="ca-tag">No key</span>}
                  </div>
                  {(projects.length > 0 || bounded.length > 0) && (
                    <dl className="ca-kv">
                      {projects.length > 0 && (
                        <>
                          <dt>Projects</dt>
                          <dd>{projects.join(', ')}</dd>
                        </>
                      )}
                      {bounded.length > 0 && (
                        <>
                          <dt>Boundary groups</dt>
                          <dd>{bounded.join(', ')}</dd>
                        </>
                      )}
                    </dl>
                  )}
                  <label className="ca-field">
                    <span>When it fails for now</span>
                    <ProfileSelect
                      value={m.fallback ?? ''}
                      profiles={[{ id: '', name: 'Try again later' }, ...models.filter((x) => x.name !== m.name).map((x) => ({ id: x.name, name: `Ask ${x.name} instead` }))]}
                      onChange={(fallback) => change(m.name, { fallback: fallback || undefined })}
                    />
                  </label>
                  {removing === m.name ? (
                    <div className="ca-acct-confirm" role="alert">
                      <span>
                        Remove <strong>{m.name}</strong>? What chose it goes back to the default; a boundary group that chose it goes back to Jev.
                      </span>
                      <Button size="sm" onClick={() => (onSave(withoutModel(cfg, m.name)), setRemoving(null))}>
                        Remove
                      </Button>
                      <Button variant="ghost" size="sm" onClick={() => setRemoving(null)}>
                        Cancel
                      </Button>
                    </div>
                  ) : (
                    (d.default !== m.name || m.name !== 'jev') && (
                      <div className="ca-acct-actions">
                        {d.default !== m.name && (
                          <Button variant="ghost" size="sm" onClick={() => save({ default: m.name })}>
                            Make default
                          </Button>
                        )}
                        {m.name !== 'jev' && (
                          <Button variant="ghost" size="sm" onClick={() => setRemoving(m.name)}>
                            Remove
                          </Button>
                        )}
                      </div>
                    )
                  )}
                </div>
              );
            })}
            {d.default === 'none' && <p className="ca-acct-note">No default: only a project or boundary group that names a model has one.</p>}
          </div>
          <small>A fallback is asked only for what goes by the default. What a project or a boundary group chose goes to that model or to none. To pin Jev to one version, give it an entry with <code className="ca-ic">model</code> under <code className="ca-ic">[[decisions.models]]</code> in config.toml.</small>
          {adding ? (
            <form
              className="ca-ucard ca-acct-form"
              onSubmit={(e) => {
                e.preventDefault();
                if (problem) return;
                save({ models: withModels([...models, { name: adding.name.trim(), provider: adding.provider, model: adding.model.trim() }]) });
                setAdding(null);
              }}
            >
              <div className="ca-acct-form-head">
                <span className="ca-ucard-name">New decision model</span>
              </div>
              <div className="ca-field">
                <span>Provider</span>
                <Seg label="Provider" options={PROVIDERS.map((p) => ({ id: p.id, label: p.word }))} value={adding.provider} onPick={(id) => setAdding(modelDraft(provider(id) ?? PROVIDERS[0], models.map((m) => m.name)))} />
              </div>
              <label className="ca-field">
                <span>Name</span>
                <input value={adding.name} spellCheck={false} autoCapitalize="off" autoCorrect="off" onChange={(e) => setAdding({ ...adding, name: e.target.value })} />
                <small>What projects and boundary groups name it by.</small>
              </label>
              <label className="ca-field">
                <span>Model</span>
                <input value={adding.model} spellCheck={false} autoCapitalize="off" autoCorrect="off" onChange={(e) => setAdding({ ...adding, model: e.target.value })} />
              </label>
              {problem && adding.name.trim() && <p className="ca-form-error">{problem}</p>}
              <div className="ca-acct-actions">
                <Button variant="primary" size="sm" type="submit" disabled={!!problem}>
                  Add model
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setAdding(null)}>
                  Cancel
                </Button>
              </div>
            </form>
          ) : (
            <div>
              <Button size="sm" icon="plus" onClick={() => setAdding(modelDraft(PROVIDERS[1], models.map((m) => m.name)))}>
                Add model
              </Button>
            </div>
          )}
        </div>
        <label className="ca-field">
          <span>Default model</span>
          <ProfileSelect value={d.default} profiles={choices(d, false)} onChange={(def) => save({ default: def })} />
          <small>What reads a project that names no model, and anything in no project. A boundary group that names none is Jev’s, whatever this says.</small>
        </label>
        {providersInUse(d).map((p) => {
          const model = models.find((m) => m.provider === p.id) ?? { name: p.name, model: p.model };
          return <KeyField key={p.id} provider={p} label={`${p.word} API key`} model={model.model} saved={`Saved in the keychain. Test it, then turn on what you want below.`} onChanged={() => setAsked((n) => n + 1)} />;
        })}
        <JevUsage nameOf={decisionFeatureName} stamp={asked} hasKey={anyKey === true} />
        <div className="ca-field">
          <span>What to use them for</span>
          <div className="ca-jev-features">
            {DECISION_FEATURES.map((f) => (
              <label key={f.id} className={cx('ca-jev-feature', noKey && 'is-disabled')}>
                <span className="ca-check">
                  <CheckBox checked={d[f.id]} disabled={noKey} onChange={(on) => save({ [f.id]: on })} />
                  <span className="ca-jev-feature-name">{f.label}</span>
                </span>
                <span className="ca-jev-feature-does">{keysIn(f.does)}</span>
                <span className="ca-jev-feature-sends">
                  <b>Sends:</b> {f.sends}
                </span>
              </label>
            ))}
          </div>
          {noKey && <small>Save a key above to turn these on.</small>}
        </div>
      </fieldset>
    </>
  );
}
