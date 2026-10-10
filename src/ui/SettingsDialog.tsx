// Settings (⌘,): a form for everything in config.toml, in sections, and the file itself as a
// fallback. Form fields save as you leave them, each in its place in the file, so the file
// stays yours: comments and order included. Accounts keep their own add, edit and remove flow
// in AccountsSection.

import { useCallback, useEffect, useRef, useState } from 'react';
import { getConfig, onQuickShortcut, openUrl, pauseQuickShortcut, quickShortcut, jevKey, saveConfig, saveConfigText, setJevKey, setReusable, startOptions, testJev } from '../api';
import { shortcutFromKey, shortcutLabel, TAKEN } from '../quick';
import { HARNESS_WORD, MOVES_ACCOUNTS } from '../harness';
import { worktreeName } from '../lanes';
import { REUSE_WORDS } from '../reusable';
import type { ChromeProfile, Config, ConfigView, JevCfg, Lane, NewTerminal, OwnReviewsCfg, Profile, ProjectCfg, Reuse, ReviewPost, ReviewsCfg, ShortcutStatus, StartAccount, Theme, Usage, WhenLimited } from '../types';
import { AccountsSection } from './AccountsSection';
import { AgentPromptBox } from './AgentPromptBox';
import { AgentsSection } from './AgentsSection';
import { DevicesPane } from './DevicesSection';
import { ChromeProfileSelect } from './PreviewBits';
import { UpdatesFields } from './Updates';
import { Button, cx, Dialog, Icon, NavItem, Seg } from './primitives';
import { THEMES } from './ThemeSwitch';
import { errorText } from '../report';

export type SettingsSection = 'general' | 'accounts' | 'automation' | 'agents' | 'devices' | 'projects' | 'jev' | 'advanced' | 'file';

const SECTIONS: { id: SettingsSection; label: string; icon: string }[] = [
  { id: 'general', label: 'General', icon: 'settings' },
  { id: 'accounts', label: 'Accounts', icon: 'user' },
  { id: 'automation', label: 'Automation', icon: 'bolt' },
  { id: 'agents', label: 'Agents', icon: 'terminal' },
  { id: 'devices', label: 'Devices', icon: 'device-desktop' },
  { id: 'projects', label: 'Projects', icon: 'folder' },
  { id: 'jev', label: 'Jev', icon: 'sparkles' },
  { id: 'advanced', label: 'Advanced', icon: 'gauge' },
  { id: 'file', label: 'config.toml', icon: 'file' },
];

const NEW_TERMINALS: { id: NewTerminal; label: string; says: string }[] = [
  { id: 'project', label: 'Tab beside its project', says: 'A tab in the window already showing that worktree, repo or project, or a new window when none does.' },
  { id: 'tab', label: 'Tab in front window', says: 'A tab in the terminal window in front.' },
  { id: 'window', label: 'New window', says: 'A window of its own every time.' },
];

const WHEN_LIMITED: { id: WhenLimited; label: string; says: string }[] = [
  { id: 'suggest', label: 'Suggest a move', says: 'The session says which account has room, with a button that moves it there to carry on.' },
  { id: 'move', label: 'Move it', says: 'Codealong moves it to the account with room as soon as it stops, and it carries on there: in a new terminal, or as a background job if it was one.' },
];

interface SettingsDialogProps {
  section: SettingsSection;
  onSection: (s: SettingsSection) => void;
  /** The account to scroll to, from a click on its usage chip. */
  focus: string | null;
  usage: Usage[];
  /** The terminal's and editor's names, for copy. */
  apps: { terminal: string; editor: string };
  theme: Theme;
  onPickTheme: (t: Theme) => void;
  now: number;
  onClose: () => void;
  /** Opens config.toml in the editor. */
  onOpenConfig: () => void;
  /** The board's worktrees, for Projects' reusable ones. */
  lanes: Lane[];
}

export function SettingsDialog({ section, onSection, focus, usage, apps, theme, onPickTheme, now, onClose, onOpenConfig, lanes }: SettingsDialogProps) {
  const [view, setView] = useState<ConfigView | null>(null);
  const [error, setError] = useState<string | null>(null);
  // What you've typed in the config.toml section, kept while you look at another section.
  const [fileDraft, setFileDraft] = useState<FileDraft | null>(null);

  const load = useCallback(() => getConfig().then(setView).catch((e: unknown) => setError(errorText(e))), []);

  useEffect(() => {
    void load();
  }, [load]);

  // Edits made in the editor, or in another window, show up on focus.
  useEffect(() => {
    const onFocus = () => void load();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [load]);

  /** Saves the whole config, then reads it back. Returns whether it saved. */
  const save = async (next: Config): Promise<boolean> => {
    setError(null);
    // Show the change at once; the file's own version replaces it after the save.
    setView((v) => (v ? { ...v, config: next } : v));
    try {
      await saveConfig(next);
      await load();
      return true;
    } catch (e) {
      setError(errorText(e));
      await load();
      return false;
    }
  };

  const cfg = view?.config ?? null;
  const broken = view?.error ?? null;

  const gate = broken && section !== 'file' && (
    <div className="ca-settings-broken" role="alert">
      <Icon name="alert" size={14} />
      <span>
        config.toml has an error, so these settings can't be changed here until it's fixed. <code className="ca-ic">{broken}</code>
      </span>
      <Button size="sm" onClick={() => onSection('file')}>
        Open config.toml
      </Button>
    </div>
  );

  let pane: React.ReactNode = null;
  if (section === 'accounts') {
    pane = (
      <>
        <AccountsSection focus={focus} usage={usage} terminal={apps.terminal} now={now} onSaved={() => void load()} />
      </>
    );
  } else if (section === 'agents') {
    pane = <AgentsSection />;
  } else if (view === null) {
    pane = error ? <p className="ca-form-error">{error}</p> : <p className="ca-empty">Reading settings…</p>;
  } else if (section === 'general' && cfg) {
    pane = <GeneralPane cfg={cfg} view={view} theme={theme} onPickTheme={onPickTheme} disabled={!!broken} onSave={save} now={now} />;
  } else if (section === 'automation' && cfg) {
    pane = (
      <>
        <p className="ca-settings-lede">What Codealong does by itself: for your sessions when something stops them, and for your pull requests.</p>
        <ReviewsField cfg={cfg} disabled={!!broken} onSave={save} defaultPrompt={view.reviewPrompt ?? ''} usage={usage} now={now} />
        <NudgeField cfg={cfg} disabled={!!broken} onSave={save} />
        <LimitsField cfg={cfg} disabled={!!broken} onSave={save} />
      </>
    );
  } else if (section === 'devices' && cfg) {
    pane = <DevicesPane cfg={cfg} disabled={!!broken} onSave={save} now={now} />;
  } else if (section === 'projects' && cfg) {
    pane = <ProjectsPane cfg={cfg} chromeProfiles={view.chromeProfiles ?? []} disabled={!!broken} onSave={save} lanes={lanes} />;
  } else if (section === 'jev' && cfg) {
    pane = <JevPane jev={{ ...JEV_OFF, ...cfg.jev }} disabled={!!broken} onChange={(jev) => void save({ ...cfg, jev })} />;
  } else if (section === 'advanced' && cfg) {
    pane = <AdvancedPane cfg={cfg} view={view} disabled={!!broken} onSave={save} />;
  } else if (section === 'file') {
    pane = <FilePane view={view} draft={fileDraft} onDraft={setFileDraft} editor={apps.editor} onOpenConfig={onOpenConfig} onSaved={() => void load()} />;
  }

  return (
    <Dialog
      className="ca-settings"
      title="Settings"
      onClose={onClose}
      aside={
        <span className="ca-settings-path" title={view?.path}>
          {view?.path ?? ''}
        </span>
      }
      foot={
        <Button variant="primary" size="sm" onClick={onClose}>
          Done
        </Button>
      }
    >
      <div className="ca-settings-body">
        <nav className="ca-settings-nav" aria-label="Settings sections">
          {SECTIONS.map((s) => (
            <NavItem key={s.id} label={s.label} icon={s.icon} active={section === s.id} onClick={() => onSection(s.id)} />
          ))}
        </nav>
        <div className={section === 'file' ? 'ca-settings-pane is-file' : 'ca-settings-pane'}>
          {gate}
          {pane}
          {error && section !== 'accounts' && section !== 'agents' && view !== null && (
            <p className="ca-form-error" role="alert">
              {error}
            </p>
          )}
        </div>
      </div>
    </Dialog>
  );
}

/* ---- General ---- */

interface PaneProps {
  cfg: Config;
  disabled: boolean;
  onSave: (next: Config) => Promise<boolean>;
}

function GeneralPane({ cfg, view, theme, onPickTheme, disabled, onSave, now }: PaneProps & { view: ConfigView; theme: Theme; onPickTheme: (t: Theme) => void; now: number }) {
  const searchDays = cfg.search?.days ?? MAX_SEARCH_DAYS;
  return (
    <>
      <div className="ca-settings-intro">
        <h3>General</h3>

        <p>Where Codealong looks for repos, which apps it opens things in, and how it looks.</p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        <div className="ca-field">
          <span>Folders searched for repos</span>
          <ListField items={cfg.scan} placeholder="~/projects" addLabel="Add folder" onChange={(scan) => void onSave({ ...cfg, scan })} />
          <small>Three levels deep. Every repo and worktree found here is on the board; projects group them below.</small>
        </div>
        <div className="ca-field">
          <span>Quick panel</span>
          <ShortcutField
            which="quick"
            name="Quick panel shortcut"
            fallback={DEFAULT_QUICK_PANEL}
            value={cfg.shortcuts?.quick_panel ?? DEFAULT_QUICK_PANEL}
            other={{ value: cfg.shortcuts?.waiting ?? DEFAULT_WAITING, name: 'the shortcut for what waits on you' }}
            onChange={(quick_panel) => void onSave({ ...cfg, shortcuts: { ...cfg.shortcuts, quick_panel } })}
          />
          <small>Opens from any app, over full-screen ones too: the sessions that need you, a search, and a new session. ⌘N does the same in Codealong.</small>
        </div>
        <div className="ca-field">
          <span>What waits on you</span>
          <ShortcutField
            which="waiting"
            name="Shortcut for what waits on you"
            fallback={DEFAULT_WAITING}
            value={cfg.shortcuts?.waiting ?? DEFAULT_WAITING}
            other={{ value: cfg.shortcuts?.quick_panel ?? DEFAULT_QUICK_PANEL, name: 'the quick panel’s shortcut' }}
            onChange={(waiting) => void onSave({ ...cfg, shortcuts: { ...cfg.shortcuts, waiting } })}
          />
          <small>Opens the first session that waits on you, from any app, to answer one after another: in the notch when the bar is docked there, else in a window of its own. Press it again to put it away.</small>
        </div>
        <label className="ca-field">
          <span>Search what sessions said</span>
          <ProfileSelect value={String(searchDays)} profiles={dayOptions(searchDays)} onChange={(d) => void onSave({ ...cfg, search: { days: Number(d) } })} />
          <small>The quick panel and <code>codealong sessions find</code> look for your words in the prompts and replies of sessions active this recently, not just in their titles. Tool output is left out. Codealong keeps this text in memory, a few megabytes a week.</small>
        </label>
        <label className="ca-field">
          <span>Terminal</span>
          <ProfileSelect value={cfg.apps.terminal} profiles={view.terminals} onChange={(terminal) => void onSave({ ...cfg, apps: { ...cfg.apps, terminal } })} />
          <small>Where sessions open and new ones start.</small>
        </label>
        <div className="ca-field">
          <span>New terminals open as</span>
          <Seg
            className="ca-settings-theme"
            label="New terminals open as"
            options={NEW_TERMINALS}
            value={cfg.apps.new_terminal ?? 'project'}
            onPick={(id) => id !== (cfg.apps.new_terminal ?? 'project') && void onSave({ ...cfg, apps: { ...cfg.apps, new_terminal: id } })}
          />
          <small>
            {cfg.apps.terminal === 'terminal'
              ? 'Terminal can’t open a tab for another app, so new terminals always open as windows there.'
              : NEW_TERMINALS.find((t) => t.id === (cfg.apps.new_terminal ?? 'project'))?.says}
          </small>
        </div>
        <label className="ca-field ca-settings-check">
          <input type="checkbox" checked={cfg.apps.move_pointer ?? false} onChange={(e) => void onSave({ ...cfg, apps: { ...cfg.apps, move_pointer: e.target.checked } })} />
          <span>Move the pointer to a terminal on another screen</span>
          <small>When a session's terminal comes forward on another screen than the pointer's, Codealong outlines it for a moment. This moves the pointer there too.</small>
        </label>
        <label className="ca-field">
          <span>Editor</span>
          <ProfileSelect value={cfg.apps.editor} profiles={view.editors} onChange={(editor) => void onSave({ ...cfg, apps: { ...cfg.apps, editor } })} />
          <small>Opens a worktree's folder, and config.toml. The Open in menus offer the other editors installed here, and picking one there makes it this.</small>
        </label>
        <div className="ca-field">
          <span>Appearance</span>
          <Seg className="ca-settings-theme" label="Theme" look="text" options={THEMES} value={theme} onPick={(t) => theme !== t && onPickTheme(t)} />
          <small>Every window follows, and so do the window chrome and menus.</small>
        </div>
        <UpdatesFields cfg={cfg} onSave={(next) => void onSave(next)} now={now} />
      </fieldset>
    </>
  );
}

/** `[limits] when_hit`: what's done about a session its account's usage limit stopped; and
 *  `[sessions] continue_after_network_error`, about one a lost connection stopped. */
function LimitsField({ cfg, disabled, onSave }: PaneProps) {
  const value = cfg.limits?.when_hit ?? 'suggest';
  const agents = MOVES_ACCOUNTS.map((h) => HARNESS_WORD[h]).join(', ');
  const continues = cfg.sessions?.continue_after_network_error ?? true;
  return (
    <fieldset className="ca-settings-fields" disabled={disabled}>
      <div className="ca-field">
        <span>When a session hits its usage limit</span>
        <Seg className="ca-settings-theme" label="When a session hits its usage limit" options={WHEN_LIMITED} value={value} onPick={(id) => id !== value && void onSave({ ...cfg, limits: { when_hit: id } })} />
        <small>
          {WHEN_LIMITED.find((w) => w.id === value)?.says} It goes to another account of its agent ({agents}) that has room by its meters and by what its own sessions last heard. The first reply there reads the whole conversation again, since a prompt cache is an account’s own.
        </small>
      </div>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={continues} onChange={(e) => void onSave({ ...cfg, sessions: { continue_after_network_error: e.target.checked } })} />
        <span>Carry on after a network error</span>
        <small>When a session’s turn ends because it couldn’t reach its API, as happens when the Mac sleeps with the lid closed or the network drops, Codealong tells it to carry on once the API can be reached again. Claude Code sessions, for now.</small>
      </label>
    </fieldset>
  );
}

/** `[reviews]` when config.toml has none (config.rs `ReviewsCfg`). */
const OWN_REVIEWS_OFF: Required<OwnReviewsCfg> = { auto: false, account: '', model: '', prompt: '', send: true };
const REVIEWS_OFF: Required<ReviewsCfg> = { auto: false, account: '', model: '', prompt: '', post: 'draft', own: OWN_REVIEWS_OFF };

/** Where each review goes (config.rs `ReviewPost`), as the select offers it. */
const REVIEW_POSTS: (Profile & { id: ReviewPost; says: string })[] = [
  { id: 'draft', name: 'A draft review for you to finish', says: 'A review on your GitHub account with its comments on the lines they’re about, left as a draft that only you see. You finish it on GitHub, approving or asking for changes, and your review is still asked for until you do.' },
  { id: 'comment', name: 'A comment on the pull request', says: 'A comment from your GitHub account that says an agent wrote it and which commit it read, each line comment under a link to its line. A comment leaves your review still asked for.' },
  { id: 'none', name: 'Nowhere: it stays in its session', says: 'Each review is in its session for you to read; nothing goes on GitHub.' },
];

/** Where an account's reviews would run, as autoreview.rs `reviewer_place` picks: unseen under
 *  its harness's background service, else in Codealong, else in a terminal behind your windows.
 *  Null for an account that can start none of those. */
function reviewsRunIn(a: StartAccount): string | null {
  const background = a.places.find((p) => p.place === 'background');
  if (background && !background.behind) return 'in the background';
  if (a.places.some((p) => p.place === 'codealong')) return 'in Codealong';
  return background ? 'in a terminal behind your windows' : null;
}

/** `[reviews]`: an agent reviews the pull requests that wait on your review, on the account and
 *  model picked, told what to do in the box under them (AgentPromptBox.tsx; Codealong's own
 *  instructions until you change them: `defaultPrompt`), its review put where you say
 *  (autoreview.rs). Only accounts that can start a session out of the way are offered. */
function ReviewsField({ cfg, disabled, onSave, defaultPrompt, usage, now }: PaneProps & { defaultPrompt: string; usage: Usage[]; now: number }) {
  const reviews = { ...REVIEWS_OFF, ...cfg.reviews };
  const [accounts, setAccounts] = useState<StartAccount[]>([]);
  useEffect(() => {
    // Without them the box says there are no accounts, and config.toml's choice stays as it is.
    startOptions()
      .then((o) => setAccounts(o.accounts.filter((a) => reviewsRunIn(a))))
      .catch(() => {});
  }, []);
  const set = (next: Partial<ReviewsCfg>) => void onSave({ ...cfg, reviews: { ...reviews, ...next } });
  const runsIn = accounts.find((a) => a.name === reviews.account);
  const post = REVIEW_POSTS.find((p) => p.id === reviews.post) ?? REVIEW_POSTS[0];
  return (
    <fieldset className="ca-settings-fields" disabled={disabled}>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={reviews.auto} onChange={(e) => set({ auto: e.target.checked })} />
        <span>Review pull requests that wait on your review</span>
        <small>
          An agent reviews each open pull request that asks for your review, in a session of its own, and again when new commits come in while it still waits on you. It reads the pull request and the code around it and changes nothing. A pull request joins what waits on you once its
          review is done, and its card says where that stands.
        </small>
      </label>
      {reviews.auto && (
        <>
          <div className="ca-field">
            <span>The reviewer</span>
            <AgentPromptBox
              accounts={accounts}
              usage={usage}
              now={now}
              account={reviews.account}
              model={reviews.model}
              prompt={reviews.prompt}
              defaultPrompt={defaultPrompt}
              label="What the reviewer is told"
              hint="What to look for, and how"
              autoTitle="The account the quick panel’s Auto would start, picked again for each pull request: a preferred one with room, the one whose week resets soonest. Reviews keep to accounts that can start a session out of the way."
              defaultLabel="Use Codealong’s instructions"
              disabled={disabled}
              onAgent={(account, model) => set({ account, model })}
              onPrompt={(prompt) => set({ prompt })}
            />
            <small>
              What it’s told: what to look for and how. It can start subagents of its own and combine what they find; in Claude Code, ask for the Task tool and name each one’s model. Codealong still tells it which pull request, where it’s written out, to change nothing, and how to lay out
              its last message so the review can be posted. {runsIn ? `It runs ${reviewsRunIn(runsIn)}.` : 'Auto picks an account that can start a session out of the way.'} This is apart from the account new sessions start on.
            </small>
          </div>
          <label className="ca-field">
            <span>Where each review goes</span>
            <ProfileSelect value={reviews.post} profiles={REVIEW_POSTS} onChange={(id) => set({ post: id as ReviewPost })} />
            <small>{post.says}</small>
          </label>
        </>
      )}
      <OwnReviewsField reviews={reviews} accounts={accounts} defaultPrompt={defaultPrompt} usage={usage} now={now} disabled={disabled} onSave={set} />
    </fieldset>
  );
}

/** `[reviews.own]`: the pull requests your sessions open, each reviewed by an agent on another
 *  model or provider than the session that opened it, its review kept here and handed back to
 *  that session (autoreview.rs). The reviewer is the same box as the one above. */
function OwnReviewsField({ reviews, accounts, defaultPrompt, usage, now, disabled, onSave }: { reviews: Required<ReviewsCfg>; accounts: StartAccount[]; defaultPrompt: string; usage: Usage[]; now: number; disabled?: boolean; onSave: (next: Partial<ReviewsCfg>) => void }) {
  const own = { ...OWN_REVIEWS_OFF, ...reviews.own };
  const set = (next: Partial<OwnReviewsCfg>) => onSave({ own: { ...own, ...next } });
  const runsIn = accounts.find((a) => a.name === own.account);
  return (
    <>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={own.auto} onChange={(e) => set({ auto: e.target.checked })} />
        <span>Review the pull requests your sessions open</span>
        <small>
          An agent on another model reviews each open pull request a session of yours opened here, in a session of its own, and again when new commits come in. It reads the pull request and the code around it and changes nothing. Its review stays in Codealong, in the pull request’s Review
          tab: nothing goes on GitHub.
        </small>
      </label>
      {own.auto && (
        <>
          <div className="ca-field">
            <span>Reviewer of your pull requests</span>
            <AgentPromptBox
              accounts={accounts}
              usage={usage}
              now={now}
              account={own.account}
              model={own.model}
              prompt={own.prompt}
              defaultPrompt={defaultPrompt}
              label="What the reviewer of your pull requests is told"
              hint="What to look for, and how"
              autoTitle="An account of another harness than the session that opened each pull request, picked as the quick panel’s Auto picks among them, so another model reads what one wrote. One of the same harness only when no other can start."
              autoSays="another harness"
              defaultLabel="Use Codealong’s instructions"
              disabled={disabled}
              onAgent={(account, model) => set({ account, model })}
              onPrompt={(prompt) => set({ prompt })}
            />
            <small>
              {runsIn ? `It runs ${reviewsRunIn(runsIn)}, whatever opened the pull request.` : 'Auto picks an account of another harness than the session that opened each pull request: Codex reviews what Claude Code wrote, say.'} Pick one with its model to have that review every one.
            </small>
          </div>
          <label className="ca-field ca-settings-check">
            <input type="checkbox" checked={own.send} onChange={(e) => set({ send: e.target.checked })} />
            <span>Hand each review to the session that opened the pull request</span>
            <small>
              Once that session runs here and doesn’t wait on you, it’s sent a message naming the review, kept as a file it reads, to fix what holds up and say what it leaves. A review that finds nothing to raise isn’t sent, nor one of commits it has moved on from. Off, send one from the pull
              request’s Review tab.
            </small>
          </label>
        </>
      )}
    </>
  );
}

/** `[pull_requests] nudge_by_default`: every open PR of yours tells the session that opened it
 *  what happens on it, unless its own switch is off (nudges.rs). */
function NudgeField({ cfg, disabled, onSave }: PaneProps) {
  const on = cfg.pull_requests?.nudge_by_default ?? false;
  return (
    <fieldset className="ca-settings-fields" disabled={disabled}>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={on} onChange={(e) => void onSave({ ...cfg, pull_requests: { nudge_by_default: e.target.checked } })} />
        <span>Nudge the session that opened a pull request</span>
        <small>
          For every open pull request of yours, the session that opened it is sent a message, as one of yours, when someone else reviews it, asks for changes or comments, when it’s approved, merged or closed, and when its checks fail, so it takes it from there. The message never asks it to merge: it merges only if you asked it to. Its other sessions aren’t told. A pull
          request’s own switch, on its card, wins over this. Off, a pull request nudges nobody until you turn it on there, or an agent does with <code>codealong pr nudge</code>.
        </small>
      </label>
    </fieldset>
  );
}

/** config.rs's DEFAULT_QUICK_PANEL. */
const DEFAULT_QUICK_PANEL = 'Option+Cmd+/';
/** config.rs's DEFAULT_WAITING. */
const DEFAULT_WAITING = 'Option+Cmd+.';

/** The board goes back this far, so searching further finds nothing more (config.rs). */
const MAX_SEARCH_DAYS = 14;

/** The choices for how far back text search reaches, with a value typed in config.toml too. */
function dayOptions(current: number): Profile[] {
  const days = [...new Set([0, 1, 3, 7, MAX_SEARCH_DAYS, current])].sort((a, b) => a - b);
  return days.map((d) => ({ id: String(d), name: d === 0 ? 'Off: titles only' : d === 1 ? 'The last day' : `The last ${d} days` }));
}

const JEV_OFF: JevCfg = { model: 'jev-latest', quick_panel: false, sessions: false, todos: false, pull_requests: false };

type JevFeature = 'quick_panel' | 'sessions' | 'todos' | 'pull_requests';

/** Each switch: what it does, and what it sends to TypeSafe to do it. */
const JEV_FEATURES: { id: JevFeature; label: string; does: string; sends: string }[] = [
  {
    id: 'quick_panel',
    label: 'Quick panel suggestions',
    does: 'As you type, suggests the session to send it to or the worktree to start it in; ⏎ does the first. Warns when a running session may already be on the same work, puts the sessions it thinks you mean first in the search, and, when Auto starts Claude Code, a smaller model for a small change.',
    sends: 'What you type, and the project’s session titles and latest lines, branches, pull request titles and worktree names. Once each time you pause typing.',
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
    sends: 'Your open todos’ titles and descriptions, your label and project names, new sessions’ titles and first prompts, and linked sessions’ latest replies with their pull requests’ titles and states.',
  },
  {
    id: 'pull_requests',
    label: 'Pull request reviews',
    does: 'Lists a pull request’s comments in its panel and marks the ones that ask the author for a change or an answer, so you can see what still needs a reply.',
    sends: 'The pull request’s title and others’ comments on threads still open, when you open it in Codealong. Answers are kept, so only new or edited comments are sent again.',
  },
];

/**
 * Settings › Jev: the TypeSafe key, which goes to the keychain and never comes back to the page,
 * and a switch per feature that says what it does and what it sends.
 */
function JevPane({ jev, disabled, onChange }: { jev: JevCfg; disabled: boolean; onChange: (j: JevCfg) => void }) {
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [editing, setEditing] = useState(false);
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    void jevKey().then(setHasKey);
  }, []);

  const run = async (what: () => Promise<string | null>) => {
    setBusy(true);
    setNote(null);
    try {
      const said = await what();
      if (said) setNote({ ok: true, text: said });
    } catch (e) {
      setNote({ ok: false, text: errorText(e) });
    }
    setHasKey(await jevKey());
    setBusy(false);
  };

  const saveKey = () =>
    run(async () => {
      await setJevKey(key);
      setKey('');
      setEditing(false);
      return 'Saved in the keychain. Test it, then turn on what you want below.';
    });

  const noKey = hasKey === false;

  return (
    <>
      <div className="ca-settings-intro">
        <h3>Jev</h3>
        <p>
          TypeSafe’s Jev answers narrow questions in a fraction of a second: which session a prompt is for, whether a reply asks you something, which todo a session is for. Each feature is off until you turn it on,
          and each says what it sends to TypeSafe. It costs $0.042 per million tokens sent; a question is a few thousand tokens, so a hundredth of a cent or less.
        </p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        <div className="ca-field">
          <span>API key</span>
          {editing || noKey ? (
            <form
              className="ca-shortcut-row"
              onSubmit={(e) => {
                e.preventDefault();
                void saveKey();
              }}
            >
              <input type="password" className="ca-jev-key" value={key} placeholder="Paste your TypeSafe API key" autoComplete="off" spellCheck={false} aria-label="TypeSafe API key" onChange={(e) => setKey(e.target.value)} />
              <Button size="sm" type="submit" disabled={busy || !key.trim()}>
                Save key
              </Button>
              {editing && (
                <Button variant="ghost" size="sm" onClick={() => setEditing(false)}>
                  Cancel
                </Button>
              )}
            </form>
          ) : hasKey ? (
            <div className="ca-shortcut-row">
              <span className="ca-jev-saved">
                <Icon name="lock" size={12} />
                Saved in the keychain
              </span>
              <Button variant="ghost" size="sm" disabled={busy} onClick={() => void run(() => testJev(jev.model))}>
                Test
              </Button>
              <Button variant="ghost" size="sm" disabled={busy} onClick={() => setEditing(true)}>
                Replace
              </Button>
              <Button variant="ghost" size="sm" disabled={busy} onClick={() => void run(async () => (await setJevKey(''), null))}>
                Remove
              </Button>
            </div>
          ) : (
            <p className="ca-empty">Reading the keychain…</p>
          )}
          {note && <p className={note.ok ? 'ca-jev-ok' : 'ca-form-error'}>{note.text}</p>}
          <small>
            Kept in your login keychain, never in config.toml.{' '}
            <button type="button" className="ca-linkbtn" onClick={() => void openUrl('https://console.typesafe.ai/keys')}>
              Get a key from TypeSafe
            </button>
          </small>
        </div>
        <div className="ca-field">
          <span>What to use it for</span>
          <div className="ca-jev-features">
            {JEV_FEATURES.map((f) => (
              <label key={f.id} className={cx('ca-jev-feature', noKey && 'is-disabled')}>
                <span className="ca-check">
                  <input type="checkbox" checked={jev[f.id]} disabled={noKey} onChange={(e) => onChange({ ...jev, [f.id]: e.target.checked })} />
                  <span className="ca-check-box" aria-hidden="true">
                    <Icon name="check" size={11} stroke={2.5} />
                  </span>
                  <span className="ca-jev-feature-name">{f.label}</span>
                </span>
                <span className="ca-jev-feature-does">{f.does}</span>
                <span className="ca-jev-feature-sends">
                  <b>Sends:</b> {f.sends}
                </span>
              </label>
            ))}
          </div>
          {noKey && <small>Save a key above to turn these on.</small>}
        </div>
        <div className="ca-field">
          <span>Model</span>
          <code className="ca-ic ca-jev-model">{jev.model}</code>
          <small>
            <code className="ca-ic">jev-latest</code> follows TypeSafe’s newest release. To stay on one version, set <code className="ca-ic">model</code> under <code className="ca-ic">[jev]</code> in config.toml.
          </small>
        </div>
      </fieldset>
    </>
  );
}

interface ShortcutFieldProps {
  /** Which of the shortcuts it is, for what the engine says of it. */
  which: ShortcutStatus['which'];
  /** What it's called, for the button's label. */
  name: string;
  /** Its default, which a button puts back. */
  fallback: string;
  value: string;
  /** The other shortcut, which this one can't be too. */
  other: { value: string; name: string };
  onChange: (shortcut: string) => void;
}

/**
 * One of the shortcuts that work in every app: shown the way macOS writes shortcuts, and recorded
 * by pressing it. While it records, the shortcuts are off, so pressing one records it instead of
 * doing what it does. Engine-side problems (a shortcut macOS won't take) show under it, and so
 * does what else on a Mac is known to use the same keys.
 */
function ShortcutField({ which, name, fallback, value, other, onChange }: ShortcutFieldProps) {
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState('');
  const [problem, setProblem] = useState<string | null>(null);
  const [status, setStatus] = useState<ShortcutStatus | null>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    void quickShortcut(which).then(setStatus);
    return onQuickShortcut((s) => s.which === which && setStatus(s));
  }, [which]);

  const record = (on: boolean) => {
    setRecording(on);
    setHeld('');
    setProblem(null);
    void pauseQuickShortcut(on);
  };
  // Leaving Settings mid-recording turns the shortcut back on.
  useEffect(() => () => void pauseQuickShortcut(false), []);

  const label = shortcutLabel(value);
  const taken = label ? TAKEN[label] : undefined;
  // The engine's word on the shortcut it has, once that's the one saved here.
  const engineError = status && status.shortcut === value.trim() ? status.error : null;

  return (
    <div className="ca-shortcut-field">
      <div className="ca-shortcut-row">
        <button
          ref={button}
          type="button"
          className={cx('ca-shortcut', recording && 'is-recording', !label && 'is-off')}
          aria-label={recording ? 'Press the new shortcut' : `${name}: ${label ?? 'off'}. Press to change.`}
          onClick={() => record(!recording)}
          onBlur={() => recording && record(false)}
          onKeyDown={(e) => {
            if (!recording) return;
            e.preventDefault();
            e.stopPropagation();
            if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
              record(false);
              return;
            }
            const made = shortcutFromKey(e);
            if (!made) {
              setHeld([e.ctrlKey && '⌃', e.altKey && '⌥', e.shiftKey && '⇧', e.metaKey && '⌘'].filter(Boolean).join(''));
              return;
            }
            if ('error' in made) {
              setProblem(made.error);
              return;
            }
            if (shortcutLabel(made.shortcut) === shortcutLabel(other.value)) {
              setProblem(`${shortcutLabel(made.shortcut)} is ${other.name}. Pick another.`);
              return;
            }
            record(false);
            onChange(made.shortcut);
          }}
          onKeyUp={(e) => recording && setHeld([e.ctrlKey && '⌃', e.altKey && '⌥', e.shiftKey && '⇧', e.metaKey && '⌘'].filter(Boolean).join(''))}
        >
          {recording ? held || 'Press a shortcut…' : (label ?? 'Off')}
        </button>
        {recording ? (
          <Button variant="ghost" size="sm" onMouseDown={(e) => e.preventDefault()} onClick={() => record(false)}>
            Cancel
          </Button>
        ) : (
          <>
            <Button variant="ghost" size="sm" onClick={() => button.current?.click()}>
              Change
            </Button>
            {label && (
              <Button variant="ghost" size="sm" onClick={() => onChange('')}>
                Turn off
              </Button>
            )}
            {value !== fallback && shortcutLabel(fallback) !== shortcutLabel(other.value) && (
              <Button variant="ghost" size="sm" onClick={() => onChange(fallback)}>
                Use {shortcutLabel(fallback)}
              </Button>
            )}
          </>
        )}
      </div>
      {problem && <p className="ca-form-error">{problem}</p>}
      {!problem && engineError && <p className="ca-form-error">{engineError}</p>}
      {!problem && !engineError && taken && (
        <p className="ca-shortcut-taken">
          <Icon name="alert" size={12} />
          <span>
            {label} is also {taken}, so only one of them will get it. Pick another, or change that one.
          </span>
        </p>
      )}
    </div>
  );
}

/** A select over the built-in profiles. A value the file names that isn't one stays listed,
 *  so a hand-edited setting shows as written instead of jumping to another app. */
function ProfileSelect({ value, profiles, onChange }: { value: string; profiles: Profile[]; onChange: (id: string) => void }) {
  const options = profiles.some((p) => p.id === value) ? profiles : [...profiles, { id: value, name: value }];
  return (
    <div className="ca-start-select ca-settings-select">
      <select value={value} onChange={(e) => e.target.value !== value && onChange(e.target.value)}>
        {options.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
      <Icon name="chevron-down" size={12} />
    </div>
  );
}

/* ---- Projects ---- */

interface ProjectDraft {
  /** The project being edited, or null when adding one. */
  original: string | null;
  name: string;
  repos: string[];
  folders: string[];
  /** '' opens its previews in the default browser. */
  chromeProfile: string;
}

function ProjectsPane({ cfg, chromeProfiles, disabled, onSave, lanes }: PaneProps & { chromeProfiles: ChromeProfile[]; lanes: Lane[] }) {
  const [draft, setDraft] = useState<ProjectDraft | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const commit = async (projects: ProjectCfg[]) => {
    setBusy(true);
    const ok = await onSave({ ...cfg, projects });
    setBusy(false);
    return ok;
  };

  const submit = async () => {
    if (!draft) return;
    const entry: ProjectCfg = { name: draft.name.trim(), repos: draft.repos, folders: draft.folders, ...(draft.chromeProfile.trim() ? { chrome_profile: draft.chromeProfile.trim() } : {}) };
    const next = draft.original ? cfg.projects.map((p) => (p.name === draft.original ? entry : p)) : [...cfg.projects, entry];
    if (await commit(next)) setDraft(null);
  };

  const remove = async (name: string) => {
    if (await commit(cfg.projects.filter((p) => p.name !== name))) setRemoving(null);
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
        <span className="ca-ucard-name">{draft.original ? `Edit ${draft.original}` : 'New project'}</span>
      </div>
      <label className="ca-field">
        <span>Name</span>
        <input value={draft.name} autoFocus spellCheck={false} autoCorrect="off" onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
        <small>What the project switcher shows.</small>
      </label>
      <div className="ca-field">
        <span>Repos</span>
        <ListField items={draft.repos} placeholder="github.com/owner/*" addLabel="Add repo" onChange={(repos) => setDraft({ ...draft, repos })} />
        <small>
          <code className="ca-ic">github.com/owner/*</code> for every repo of an owner, or <code className="ca-ic">github.com/owner/name</code> for one.
        </small>
      </div>
      <div className="ca-field">
        <span>Folders</span>
        <ListField items={draft.folders} placeholder="~/projects/notes" addLabel="Add folder" onChange={(folders) => setDraft({ ...draft, folders })} />
        <small>Folders that aren't repos but belong to the project.</small>
      </div>
      {(chromeProfiles.length > 0 || draft.chromeProfile) && (
        <label className="ca-field">
          <span>Chrome profile</span>
          <ChromeProfileSelect value={draft.chromeProfile} profiles={chromeProfiles} none="None: the default browser" onChange={(chromeProfile) => setDraft({ ...draft, chromeProfile })} />
          <small>Its worktrees’ previews, and links to them in conversations, open in this profile, with its sign-in and cookies. A worktree can pick its own in its Preview URL sheet.</small>
        </label>
      )}
      <div className="ca-acct-actions">
        <Button variant="primary" size="sm" type="submit" disabled={busy || !draft.name.trim()}>
          {draft.original ? 'Save' : 'Add project'}
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
        <h3>Projects</h3>
        <p>A project groups repos by pattern, plus any plain folders. Repos no project claims show under Elsewhere.</p>
      </div>
      <fieldset className="ca-settings-cards" disabled={disabled}>
        {cfg.projects.map((p) => {
          if (draft?.original === p.name) return <div key={p.name}>{form}</div>;
          return (
            <div key={p.name} className="ca-ucard ca-acct">
              <div className="ca-ucard-head">
                <span className="ca-ucard-name">{p.name}</span>
              </div>
              <dl className="ca-kv">
                <dt>Repos</dt>
                <dd className="ca-code">{p.repos.length ? p.repos.join(', ') : 'None'}</dd>
                <dt>Folders</dt>
                <dd className="ca-code">{p.folders.length ? p.folders.join(', ') : 'None'}</dd>
                {p.chrome_profile && (
                  <>
                    <dt>Previews</dt>
                    <dd>Chrome · {p.chrome_profile}</dd>
                  </>
                )}
              </dl>
              {removing === p.name ? (
                <div className="ca-acct-confirm" role="alert">
                  <span>
                    Remove <strong>{p.name}</strong>? Its repos move to Elsewhere. Nothing is deleted from disk.
                  </span>
                  <Button size="sm" disabled={busy} onClick={() => void remove(p.name)}>
                    Remove
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setRemoving(null)}>
                    Cancel
                  </Button>
                </div>
              ) : (
                <div className="ca-acct-actions">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setRemoving(null);
                      setDraft({ original: p.name, name: p.name, repos: p.repos, folders: p.folders, chromeProfile: p.chrome_profile ?? '' });
                    }}
                  >
                    Edit
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setRemoving(p.name)}>
                    Remove
                  </Button>
                </div>
              )}
            </div>
          );
        })}
        {cfg.projects.length === 0 && !draft && <p className="ca-empty">No projects yet. Everything shows under Elsewhere until you add one.</p>}
        {draft && !draft.original && form}
        {!draft && (
          <div className="ca-acct-add">
            <Button
              icon="plus"
              onClick={() => {
                setRemoving(null);
                setDraft({ original: null, name: '', repos: [], folders: [], chromeProfile: '' });
              }}
            >
              Add project
            </Button>
          </div>
        )}
      </fieldset>
      <ReusableWorktrees lanes={lanes} />
    </>
  );
}

const REUSE_STATE: Record<Reuse, string> = { free: 'Ready for new work', open: 'Work under way', review: 'In review', landed: 'Landed' };

/**
 * Worktrees that take one piece of work after another, by project, and ones whose branch has
 * carried several merged PRs as a reused worktree's does (board::looks_reusable), to make
 * reusable with a click. Marks are kept in worktrees.json, not config.toml (reusable.rs), and a
 * worktree's menu changes them too.
 */
function ReusableWorktrees({ lanes }: { lanes: Lane[] }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mine = lanes.filter((l) => !l.device && l.project !== 'Elsewhere');
  const suggested = mine.filter((l) => l.looksReusable);
  const byProject = new Map<string, Lane[]>();
  for (const l of mine) if (l.reuse) byProject.set(l.project, [...(byProject.get(l.project) ?? []), l]);
  const mark = async (l: Lane, on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await setReusable(l.id, on);
    } catch (e) {
      setError(errorText(e));
    }
    setBusy(false);
  };
  const name = (l: Lane) => `${l.repo} · ${worktreeName(l)}`;
  // Why one is reusable: a main worktree and one that has been reused are unless marked
  // otherwise (board::reusable); the rest were marked.
  const why = (l: Lane) => (l.origin === 'main' ? 'main worktree' : (l.git?.newWorkCount ?? 0) >= 3 ? `reused ${l.git?.newWorkCount} times` : 'marked');
  return (
    <>
      <div className="ca-settings-intro">
        <h3>Reusable worktrees</h3>
        <p>
          A reusable worktree takes one piece of work after another; the rest are kept for one piece of work each. Jev suggests new work only in reusable worktrees, the ones free of work first.
          Main worktrees, and worktrees that have taken new work three times or more, are reusable unless you turn them off: a worktree's right-click menu has Reuse for new work.
        </p>
      </div>
      <div className="ca-settings-cards">
        {error && (
          <p className="ca-acct-note" role="alert">
            {error}
          </p>
        )}
        {suggested.length > 0 && (
          <div className="ca-ucard ca-acct">
            <div className="ca-ucard-head">
              <span className="ca-ucard-name">These look reusable</span>
            </div>
            <p className="ca-acct-note">Their branches have carried several merged PRs, one after another.</p>
            {suggested.map((l) => (
              <div key={l.id} className="ca-reuse-row">
                <span className="ca-code" title={l.path}>
                  {name(l)}
                </span>
                <span className="ca-acct-actions">
                  <Button size="sm" disabled={busy} onClick={() => void mark(l, true)}>
                    Reuse for new work
                  </Button>
                  <Button variant="ghost" size="sm" disabled={busy} onClick={() => void mark(l, false)}>
                    Not this one
                  </Button>
                </span>
              </div>
            ))}
          </div>
        )}
        {[...byProject].map(([project, list]) => (
          <div key={project} className="ca-ucard ca-acct">
            <div className="ca-ucard-head">
              <span className="ca-ucard-name">{project}</span>
            </div>
            {list.map((l) => (
              <div key={l.id} className="ca-reuse-row">
                <span className="ca-code" title={l.path}>
                  {name(l)}
                </span>
                <span className="ca-meta" title={l.reuse ? REUSE_WORDS[l.reuse] : undefined}>
                  {l.reuse ? `${REUSE_STATE[l.reuse]} · ${why(l)}` : ''}
                </span>
              </div>
            ))}
          </div>
        ))}
        {byProject.size === 0 && suggested.length === 0 && <p className="ca-empty">No reusable worktrees yet.</p>}
      </div>
    </>
  );
}

/* ---- Advanced ---- */

function AdvancedPane({ cfg, view, disabled, onSave }: PaneProps & { view: ConfigView }) {
  const dir = view.path.replace(/\/[^/]*$/, '');
  return (
    <>
      <div className="ca-settings-intro">
        <h3>Advanced</h3>
        <p>For finding out what's slow, and where Codealong keeps its files.</p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        <label className="ca-field ca-settings-check">
          <input type="checkbox" checked={cfg.debug.perf_log} onChange={(e) => void onSave({ ...cfg, debug: { ...cfg.debug, perf_log: e.target.checked } })} />
          <span>Performance log</span>
          <small>
            Appends timings from the engine and every window to <code className="ca-ic">~/Library/Logs/Codealong/perf.log</code>. Turn it off when you're done: the log grows.
          </small>
        </label>
        <div className="ca-field">
          <span>Files</span>
          <dl className="ca-kv ca-settings-files">
            <dt>Settings</dt>
            <dd className="ca-code">{view.path}</dd>
            <dt>Seen, pinned, archived</dt>
            <dd className="ca-code">{dir}/state.json</dd>
            <dt>Bar, floating or in the notch</dt>
            <dd className="ca-code">{dir}/bar.json</dd>
          </dl>
        </div>
      </fieldset>
    </>
  );
}

/* ---- config.toml ---- */

/** What you're typing in the config.toml section, and the file it started from. */
interface FileDraft {
  text: string;
  base: string;
}

interface FilePaneProps {
  view: ConfigView | null;
  draft: FileDraft | null;
  onDraft: (d: FileDraft | null) => void;
  editor: string;
  onOpenConfig: () => void;
  onSaved: () => void;
}

function FilePane({ view, draft, onDraft, editor, onOpenConfig, onSaved }: FilePaneProps) {
  const onDisk = view?.text ?? '';
  // An untouched editor follows the file; an edited one waits, and says so when the file moved.
  const text = draft?.text ?? onDisk;
  const base = draft?.base ?? onDisk;
  const dirty = text !== base;
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const area = useRef<HTMLTextAreaElement>(null);
  const setText = (t: string) => onDraft(t === onDisk ? null : { text: t, base });

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await saveConfigText(text);
      onDraft(null);
      onSaved();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const revert = () => {
    onDraft(null);
    setError(null);
  };

  return (
    <>
      <div className="ca-settings-intro">
        <h3>config.toml</h3>
        <p>Everything in Settings is in this file, with room for what the form doesn't cover. Codealong watches it and applies what you save.</p>
      </div>
      <div className="ca-settings-filehead">
        <span className="ca-code" title={view?.path}>
          {view?.path ?? ''}
        </span>
        <Button size="sm" icon="external" onClick={onOpenConfig}>
          Open in {editor}
        </Button>
      </div>
      <textarea
        ref={area}
        className="ca-settings-editor"
        value={text}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        aria-label="config.toml"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.metaKey && e.key.toLowerCase() === 's') {
            e.preventDefault();
            if (dirty && !busy) void save();
          }
          if (e.key === 'Escape' && dirty) {
            e.preventDefault();
            revert();
          }
        }}
      />
      <div className="ca-acct-actions">
        <Button variant="primary" size="sm" disabled={!dirty || busy} onClick={() => void save()}>
          Save
        </Button>
        <Button variant="ghost" size="sm" disabled={!dirty || busy} onClick={revert}>
          Revert
        </Button>
        {dirty && base !== onDisk && <span className="ca-acct-asof">config.toml changed on disk. Revert to see it.</span>}
        {!dirty && view?.error && <span className="ca-acct-asof">Fix the error and save; the other sections come back.</span>}
      </div>
      {(error ?? view?.error) && (
        <p className="ca-form-error" role="alert">
          {error ?? view?.error}
        </p>
      )}
    </>
  );
}

/* ---- Fields ---- */

/** A list of short strings, one text field per entry. Each entry saves when you leave it or
 *  press Enter, an emptied one is dropped, and Escape puts back what was there. */
export function ListField({ items, placeholder, addLabel, onChange }: { items: string[]; placeholder: string; addLabel: string; onChange: (items: string[]) => void }) {
  // The rows as typed. A row is added empty and joins `items` once it has text.
  const [rows, setRows] = useState<string[]>(items);
  const [added, setAdded] = useState(false);
  const key = JSON.stringify(items);
  useEffect(() => {
    setRows(items);
    setAdded(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed on what the list holds: a new array with the same entries mustn't put back a row being typed
  }, [key]);

  const commit = (next: string[]) => {
    const clean = next.map((r) => r.trim()).filter(Boolean);
    if (JSON.stringify(clean) !== key) onChange(clean);
    else setRows(clean);
  };

  return (
    <div className="ca-list-field">
      {rows.map((r, i) => (
        <div key={i} className="ca-list-row">
          <input
            value={r}
            placeholder={placeholder}
            autoFocus={added && i === rows.length - 1}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            onChange={(e) => setRows(rows.map((x, j) => (j === i ? e.target.value : x)))}
            onBlur={() => commit(rows)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                e.currentTarget.blur();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                e.stopPropagation();
                setRows(items);
                setAdded(false);
                // Blur after the rows are put back, so the blur doesn't commit the old text.
                requestAnimationFrame(() => (document.activeElement as HTMLElement | null)?.blur());
              }
            }}
          />
          <Button variant="ghost" size="sm" icon="x" aria-label={`Remove ${r || 'entry'}`} title="Remove" onMouseDown={(e) => e.preventDefault()} onClick={() => commit(rows.filter((_, j) => j !== i))} />
        </div>
      ))}
      <div>
        <Button
          variant="ghost"
          size="sm"
          icon="plus"
          onClick={() => {
            setRows([...rows, '']);
            setAdded(true);
          }}
        >
          {addLabel}
        </Button>
      </div>
    </div>
  );
}
