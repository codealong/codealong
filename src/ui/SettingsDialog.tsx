// Settings (⌘,): a form for everything in config.toml, in sections, and the file itself as a
// fallback. Form fields save as you leave them, each in its place in the file, so the file
// stays yours: comments and order included. Accounts keep their own add, edit and remove flow
// in AccountsSection.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getConfig, onQuickShortcut, openTeamFile, openUrl, pauseQuickShortcut, playSound, quickShortcut, jevKey, saveConfig, saveConfigText, setJevKey, setReusable, startOptions, takeTeamSettings, testJev } from '../api';
import { shortcutFromKey, shortcutLabel, TAKEN } from '../quick';
import { HARNESS_WORD, MOVES_ACCOUNTS } from '../harness';
import { worktreeName } from '../lanes';
import { REUSE_WORDS } from '../reusable';
import { reviewsRunIn } from '../review';
import { type Field, mineOf, offerWords, pullRequestsFor, repoChoices, reviewsEverywhere, reviewsFor, type Reviews, sourceOf, type Source, teamFileOf, withOwn, withPullRequests, withRepo, withReviews, without, PULL_REQUESTS_OFF } from '../repoSettings';
import type { ChromeProfile, Config, ConfigView, JevCfg, Lane, NewTerminal, NudgeAbout, OwnReviewsCfg, Profile, ProjectCfg, PullRequestsCfg, RepoCfg, Reuse, ReviewPost, ReviewsCfg, ReviewStart, ReviewSubmit, SessionsCfg, ShortcutStatus, SoundsCfg, StartAccount, TeamCfg, TeamFile, Theme, Usage, Verdict, WhenLimited } from '../types';
import { AccountsSection } from './AccountsSection';
import { AgentPromptBox } from './AgentPromptBox';
import { AgentsSection } from './AgentsSection';
import { DevicesPane } from './DevicesSection';
import { ChromeProfileSelect } from './PreviewBits';
import { UpdatesFields } from './Updates';
import { Button, cx, Dialog, Icon, NavItem, Seg } from './primitives';
import { THEMES } from './ThemeSwitch';
import { errorText } from '../report';

export type SettingsSection = 'general' | 'sounds' | 'accounts' | 'automation' | 'agents' | 'devices' | 'projects' | 'jev' | 'advanced' | 'file';

const SECTIONS: { id: SettingsSection; label: string; icon: string }[] = [
  { id: 'general', label: 'General', icon: 'settings' },
  { id: 'sounds', label: 'Sounds', icon: 'bell' },
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
  /** The account to scroll to, from a click on its usage chip; in Automation, the repo to show
   *  (`owner/name`), from a worktree's menu; in Projects, NEW_PROJECT opens a new project's form,
   *  from the project switcher's Add project…. */
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
  /** The board's worktrees, for Projects' reusable ones and the repos Automation can show. */
  lanes: Lane[];
  /** The repos' team files as read (teamfile.rs), for Automation. */
  teamFiles: TeamFile[];
}

export function SettingsDialog({ section, onSection, focus, usage, apps, theme, onPickTheme, now, onClose, onOpenConfig, lanes, teamFiles }: SettingsDialogProps) {
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
  } else if (section === 'sounds' && cfg) {
    pane = <SoundsPane cfg={cfg} names={view.sounds ?? []} disabled={!!broken} onSave={save} onError={setError} />;
  } else if (section === 'automation' && cfg) {
    pane = <AutomationPane cfg={cfg} disabled={!!broken} onSave={save} onReload={() => void load()} defaultPrompt={view.reviewPrompt ?? ''} usage={usage} now={now} lanes={lanes} teamFiles={teamFiles} focus={focus} />;
  } else if (section === 'devices' && cfg) {
    pane = <DevicesPane cfg={cfg} disabled={!!broken} onSave={save} now={now} />;
  } else if (section === 'projects' && cfg) {
    pane = <ProjectsPane cfg={cfg} chromeProfiles={view.chromeProfiles ?? []} disabled={!!broken} onSave={save} lanes={lanes} adding={focus === NEW_PROJECT} />;
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
            others={[
              { value: cfg.shortcuts?.waiting ?? DEFAULT_WAITING, name: 'the shortcut for what waits on you' },
              { value: cfg.shortcuts?.improve ?? DEFAULT_IMPROVE, name: 'Improve Codealong’s shortcut' },
            ]}
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
            others={[
              { value: cfg.shortcuts?.quick_panel ?? DEFAULT_QUICK_PANEL, name: 'the quick panel’s shortcut' },
              { value: cfg.shortcuts?.improve ?? DEFAULT_IMPROVE, name: 'Improve Codealong’s shortcut' },
            ]}
            onChange={(waiting) => void onSave({ ...cfg, shortcuts: { ...cfg.shortcuts, waiting } })}
          />
          <small>Opens the first session that waits on you, from any app, to answer one after another: in the notch when the bar is docked there, else in a window of its own. Press it again to put it away.</small>
        </div>
        <div className="ca-field">
          <span>Improve Codealong</span>
          <ShortcutField
            which="improve"
            name="Improve Codealong’s shortcut"
            fallback={DEFAULT_IMPROVE}
            value={cfg.shortcuts?.improve ?? DEFAULT_IMPROVE}
            others={[
              { value: cfg.shortcuts?.quick_panel ?? DEFAULT_QUICK_PANEL, name: 'the quick panel’s shortcut' },
              { value: cfg.shortcuts?.waiting ?? DEFAULT_WAITING, name: 'the shortcut for what waits on you' },
            ]}
            onChange={(improve) => void onSave({ ...cfg, shortcuts: { ...cfg.shortcuts, improve } })}
          />
          <small>Opens the quick panel on Codealong’s own project with a picture of every Codealong window as it is, to describe what to change: the session improves Codealong itself. The button beside New session does the same.</small>
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

/* ---- Sounds ---- */

/** `[sounds]` when config.toml has none (config.rs `SoundsCfg`). */
const SOUNDS_DEFAULT: SoundsCfg = { needs_you: 'Pop', your_turn: 'Tink', review_requested: '', volume: 60 };

type SoundKind = 'needs_you' | 'your_turn' | 'review_requested';

/** What each sound is for (sounds.rs `Kind`), the most urgent first. */
const SOUND_KINDS: { id: SoundKind; label: string; says: string }[] = [
  { id: 'needs_you', label: 'A session needs you', says: 'It asks you something, waits on a call to allow, or stopped with an error.' },
  { id: 'your_turn', label: 'A session’s turn ended', says: 'Its reply is yours to read.' },
  { id: 'review_requested', label: 'A pull request asks for your review', says: 'Someone asked for your review on GitHub.' },
];

/**
 * Settings › Sounds: the sound for each thing that starts to wait on you, with a button to hear
 * it, and how loud they all are (sounds.rs). The volume saves when you let go of it, and plays
 * the first sound there is at it, so you hear what you picked.
 */
function SoundsPane({ cfg, names, disabled, onSave, onError }: PaneProps & { names: string[]; onError: (e: string) => void }) {
  const sounds = { ...SOUNDS_DEFAULT, ...cfg.sounds };
  const [volume, setVolume] = useState(sounds.volume);
  useEffect(() => setVolume(sounds.volume), [sounds.volume]);
  const options: Profile[] = [{ id: '', name: 'None' }, ...names.map((n) => ({ id: n, name: n }))];
  const set = (next: Partial<SoundsCfg>) => void onSave({ ...cfg, sounds: { ...sounds, ...next } });
  const hear = (name: string, at: number) => void playSound(name, at).catch((e: unknown) => onError(errorText(e)));
  const commitVolume = () => {
    if (volume === sounds.volume) return;
    set({ volume });
    const first = SOUND_KINDS.map((k) => sounds[k.id]).find(Boolean);
    if (first && volume > 0) hear(first, volume);
  };
  return (
    <>
      <div className="ca-settings-intro">
        <h3>Sounds</h3>
        <p>A short sound when something starts to wait on you, for when you’re looking somewhere else. The bar’s mark and the notch show it too.</p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        {SOUND_KINDS.map((k) => (
          <div key={k.id} className="ca-field">
            <span>{k.label}</span>
            <div className="ca-settings-sound">
              <ProfileSelect
                value={sounds[k.id]}
                profiles={options}
                onChange={(name) => {
                  set({ [k.id]: name });
                  if (name) hear(name, volume);
                }}
              />
              <Button variant="ghost" size="sm" icon="player-play" disabled={!sounds[k.id]} aria-label={`Play ${sounds[k.id] || 'the sound'}`} title="Play it" onClick={() => hear(sounds[k.id], volume)} />
            </div>
            <small>{k.says}</small>
          </div>
        ))}
        <label className="ca-field">
          <span>Volume</span>
          <div className="ca-settings-sound">
            <input
              type="range"
              className="ca-settings-volume"
              min={0}
              max={100}
              step={5}
              value={volume}
              onChange={(e) => setVolume(Number(e.target.value))}
              onPointerUp={commitVolume}
              onKeyUp={commitVolume}
              onBlur={commitVolume}
            />
            <output className="ca-settings-volume-at">{volume}%</output>
          </div>
          <small>Of the Mac’s own volume.</small>
        </label>
        <p className="ca-settings-lede">
          To stay out of the way, nothing sounds for what already waits when Codealong opens, nor for a session that comes back to you within 15 seconds of going back to work (you just sent it something or answered it, so you’re likely watching), nor for a snoozed
          session or the end of a quieted one’s turn. A few at once make one sound, the most urgent’s, and there’s one at most every 10 seconds. Sounds you put in ~/Library/Sounds are listed too.
        </p>
      </fieldset>
    </>
  );
}

/** `[limits] when_hit`: what's done about a session its account's usage limit stopped;
 *  `[sessions] continue_after_network_error`, about one a lost connection stopped; and
 *  `[sessions] resume_for_sessions_it_started`, about one that stopped before a session it
 *  started sent it a message. */
function LimitsField({ cfg, disabled, onSave }: PaneProps) {
  const value = cfg.limits?.when_hit ?? 'suggest';
  const agents = MOVES_ACCOUNTS.map((h) => HARNESS_WORD[h]).join(', ');
  const sessions: SessionsCfg = { continue_after_network_error: true, resume_for_sessions_it_started: true, ...cfg.sessions };
  const continues = sessions.continue_after_network_error;
  const resumes = sessions.resume_for_sessions_it_started ?? true;
  const saveSessions = (change: Partial<SessionsCfg>) => void onSave({ ...cfg, sessions: { ...sessions, ...change } });
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
        <input type="checkbox" checked={continues} onChange={(e) => saveSessions({ continue_after_network_error: e.target.checked })} />
        <span>Carry on after a network error</span>
        <small>When a session’s turn ends because it couldn’t reach its API, as happens when the Mac sleeps with the lid closed or the network drops, Codealong tells it to carry on once the API can be reached again. Claude Code sessions, for now.</small>
      </label>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={resumes} onChange={(e) => saveSessions({ resume_for_sessions_it_started: e.target.checked })} />
        <span>Resume a session for one it started</span>
        <small>
          When a session’s agent started another with codealong start, and that one sends it a message once the first has stopped (Claude Code ends a background job left idle for an hour), Codealong resumes it out of sight with the message, rather than nothing being sent. Not one you archived or marked done, nor one that skips permission prompts (or may, as far as Codealong can tell) when the sender doesn’t.
        </small>
      </label>
    </fieldset>
  );
}

/** Where each review goes (config.rs `ReviewPost`), as the select offers it. */
const REVIEW_POSTS: (Profile & { id: ReviewPost; says: string })[] = [
  { id: 'draft', name: 'A draft review for you to finish', says: 'A review on your GitHub account with its comments on the lines they’re about, left as a draft that only you see. You finish it from the pull request’s box in Codealong or on GitHub, approving or asking for changes, and your review is still asked for until you do.' },
  { id: 'comment', name: 'A comment on the pull request', says: 'A comment from your GitHub account that says an agent wrote it and which commit it read, each line comment under a link to its line. A comment leaves your review still asked for.' },
  { id: 'none', name: 'Nowhere: it stays in its session', says: 'Each review is in its session for you to read; nothing goes on GitHub but what’s submitted for you (below).' },
];

/** Where a review's session starts (config.rs `ReviewStart`, autoreview.rs `review_places`), as
 *  the select offers it, and how "can't start a review …" says it. */
const REVIEW_STARTS: (Profile & { id: ReviewStart; says: string; where: string })[] = [
  { id: 'no-terminal', name: 'Never in a terminal', where: 'without a terminal', says: 'Claude Code reviews under its background service, and Codex and OpenCode in Codealong, through their Agent Client Protocol adapters: no window opens, and one running in Codealong stops if Codealong quits. Antigravity and Cursor can do neither, so they don’t review.' },
  { id: 'codealong', name: 'In Codealong', where: 'in Codealong', says: 'Every reviewer runs in Codealong, through its account’s Agent Client Protocol adapter, Claude Code too: no window opens, and a review that’s running stops if Codealong quits.' },
  { id: 'background', name: 'In the background, as each account starts there', where: 'in the background', says: 'As the quick panel’s Background: Claude Code under its background service, Codex and OpenCode in Codealong, and Antigravity and Cursor in a terminal opened behind your windows.' },
  { id: 'terminal', name: 'In your terminal', where: 'in a terminal', says: 'Each review opens in your terminal to watch it in, a tab or a window as New terminals open as says (General), and comes forward when it starts.' },
];

/** What of the agent's verdict is submitted as your review (config.rs `ReviewSubmit`,
 *  autoreview.rs `to_submit`), as the select offers it. */
const SUBMITTED_FOR_YOU = 'Your draft review goes with it, comments and all, and the pull request leaves what waits on you. When the agent can’t tell, or your draft holds words of yours, it’s left for you. Never a merge.';
const REVIEW_SUBMITS: (Profile & { id: ReviewSubmit; says: string })[] = [
  { id: 'never', name: 'Never: you submit each one', says: 'Each review ends on what the agent would do: approve, ask for changes, or comment, which the pull request’s box suggests. You submit your review yourself, and the pull request waits on you until you do.' },
  { id: 'approve', name: 'Approve when it finds nothing to fix', says: `A pull request the agent would approve (minor points at most) is approved as you once its review is posted; one it would ask for changes on is left for you. ${SUBMITTED_FOR_YOU}` },
  { id: 'approve-or-request-changes', name: 'Approve, or request changes', says: `As the agent would: a pull request it would approve is approved as you once its review is posted, and where it would ask for changes, so do you, a request for changes that can block the merge until it’s approved. ${SUBMITTED_FOR_YOU}` },
  { id: 'approve-or-comment', name: 'Approve, or comment instead of requesting changes', says: `A pull request the agent would approve is approved as you once its review is posted; where it would ask for changes, its review goes as a comment from you instead, which reaches the author without blocking the merge. ${SUBMITTED_FOR_YOU}` },
  { id: 'request-changes', name: 'Request changes, never approve', says: `Where the agent would ask for changes, a request for changes goes from you once its review is posted, which can block the merge until it’s approved. A pull request it would approve is left for you to approve yourself. ${SUBMITTED_FOR_YOU}` },
  { id: 'comment', name: 'Comment instead of requesting changes, never approve', says: `Where the agent would ask for changes, its review goes as a comment from you once it’s posted, which reaches the author without blocking the merge. A pull request it would approve is left for you to approve yourself. ${SUBMITTED_FOR_YOU}` },
];

/** Settings › Automation: what Codealong does by itself, everywhere or for one repo's pull
 *  requests (repoSettings.ts). With a repo picked, the same fields show what goes for it and where
 *  each value comes from; a change is yours for that repo (`[repos."github.com/…"]`), and Reset
 *  goes back to the team's or everywhere's. Over them, the repo's team settings, read from its
 *  default branch: what they add, and what they suggest, to take with Use them. */
function AutomationPane({ cfg, disabled, onSave, onReload, defaultPrompt, usage, now, lanes, teamFiles, focus }: PaneProps & { onReload: () => void; defaultPrompt: string; usage: Usage[]; now: number; lanes: Lane[]; teamFiles: TeamFile[]; focus: string | null }) {
  const choices = useMemo(() => repoChoices(lanes, cfg.repos, teamFiles), [lanes, cfg.repos, teamFiles]);
  const [repo, setRepo] = useState<string | null>(focus);
  // A worktree's menu asks for its repo while Settings is open.
  useEffect(() => setRepo(focus), [focus]);
  const mine = repo ? mineOf(cfg.repos, repo) : undefined;
  const file = repo ? teamFileOf(teamFiles, repo) : undefined;
  const team = file?.settings ?? undefined;
  const reviews = repo ? reviewsFor(cfg.reviews, mine) : reviewsEverywhere(cfg.reviews);
  const prs = repo ? pullRequestsFor(cfg.pull_requests, team, mine) : { ...PULL_REQUESTS_OFF, ...cfg.pull_requests };
  const saveMine = (slug: string, next: RepoCfg | undefined) => void onSave({ ...cfg, repos: withRepo(cfg.repos, slug, next) });
  const setReviews = (next: Partial<Omit<ReviewsCfg, 'own'>>) => (repo ? saveMine(repo, withReviews(mine, next)) : void onSave({ ...cfg, reviews: { ...reviews, ...next } }));
  const setOwn = (next: Partial<OwnReviewsCfg>) => (repo ? saveMine(repo, withOwn(mine, next)) : void onSave({ ...cfg, reviews: { ...reviews, own: { ...reviews.own, ...next } } }));
  const setPrs = (next: Partial<PullRequestsCfg>) => (repo ? saveMine(repo, withPullRequests(mine, next)) : void onSave({ ...cfg, pull_requests: { ...prs, ...next } }));
  const scope: Scope | undefined = repo ? { repo, source: (f) => sourceOf(f, mine, team), reset: (f) => saveMine(repo, without(mine, f)) } : undefined;
  return (
    <>
      <p className="ca-settings-lede">What Codealong does by itself: for your sessions when something stops them, and for your pull requests, everywhere or in one repo.</p>
      <RepoPicker repo={repo} choices={choices} onPick={setRepo} />
      {repo && <TeamSettings repo={repo} file={file} disabled={disabled} onTaken={onReload} />}
      <ReviewsField reviews={reviews} set={setReviews} setOwn={setOwn} scope={scope} team={team} disabled={disabled} defaultPrompt={defaultPrompt} usage={usage} now={now} />
      <NudgeField prs={prs} set={setPrs} scope={scope} disabled={disabled} />
      {repo ? <p className="ca-settings-lede">Usage limits and lost connections go the same in every repo: pick Everywhere to change them.</p> : <LimitsField cfg={cfg} disabled={disabled} onSave={onSave} />}
    </>
  );
}

/** The repo Automation shows, or none for everywhere. */
function RepoPicker({ repo, choices, onPick }: { repo: string | null; choices: { repo: string; team: boolean; mine: boolean }[]; onPick: (repo: string | null) => void }) {
  const profiles: Profile[] = [{ id: '', name: 'Everywhere' }, ...choices.map((c) => ({ id: c.repo, name: [c.repo, c.team && 'team settings', c.mine && 'yours'].filter(Boolean).join(' · ') }))];
  return (
    <div className="ca-field ca-settings-scope">
      <span>For</span>
      <ProfileSelect value={repo ?? ''} profiles={profiles} onChange={(id) => onPick(id || null)} />
      <small>{repo ? `What goes for ${repo}’s pull requests. What you change here is yours for this repo, over what goes everywhere and what its team sets.` : 'What goes for every repo, unless one has settings of its own: pick it to see them.'}</small>
    </div>
  );
}

/** A repo picked in Automation: where each value comes from, and Reset for one that's yours. */
interface Scope {
  repo: string;
  source: (field: Field) => Source;
  reset: (field: Field) => void;
}

const SOURCE_WORDS: Record<Source, string> = { repo: 'Yours here', team: 'Team', everywhere: 'Same as everywhere' };

/** Where a field's value comes from, with the repo picked; nothing for everywhere. */
function SourceTag({ field, scope }: { field: Field; scope?: Scope }) {
  if (!scope) return null;
  const from = scope.source(field);
  const title = from === 'repo' ? `Set for ${scope.repo} in your config.toml` : from === 'team' ? `${scope.repo}’s .codealong/config.toml sets this for its team` : 'As it goes everywhere';
  // The space keeps the label's words apart from the tag for a screen reader; without a repo
  // picked, the label is its words alone.
  return (
    <>
      {' '}
      <span className="ca-settings-source">
        <span className={cx('ca-settings-from', `is-${from}`)} title={title}>
          {SOURCE_WORDS[from]}
        </span>
        {from === 'repo' && (
          <button
            type="button"
            className="ca-linkbtn"
            title="Go back to what goes without your setting for this repo"
            onClick={(e) => {
              // Inside a label: the click is the button's, not the checkbox's.
              e.preventDefault();
              scope.reset(field);
            }}
          >
            Reset
          </button>
        )}
      </span>
    </>
  );
}

const VERDICT_WORDS: Record<Verdict, string> = { approve: 'an approval', 'request-changes': 'a request for changes', comment: 'a comment' };

/** A repo's team settings as its default branch has them: where they're from, why they aren't used
 *  when they aren't, and what they suggest, with Use them. */
function TeamSettings({ repo, file, disabled, onTaken }: { repo: string; file: TeamFile | undefined; disabled: boolean; onTaken: () => void }) {
  const [problem, setProblem] = useState<string | null>(null);
  const [taking, setTaking] = useState(false);
  if (!file) {
    return (
      <p className="ca-settings-lede">
        {repo} has no team settings. A <code className="ca-ic">.codealong/config.toml</code> in the repo shares them with everyone who works on it: words for every review, what an agent may submit, and what to suggest turning on.
      </p>
    );
  }
  const offers = offerWords(file.offers);
  const take = () => {
    setTaking(true);
    setProblem(null);
    takeTeamSettings(file.repo, file.branch)
      .then(onTaken)
      .catch((e: unknown) => setProblem(errorText(e)))
      .finally(() => setTaking(false));
  };
  return (
    <section className="ca-settings-team" aria-label={`${repo}’s team settings`}>
      <div className="ca-settings-team-head">
        <Icon name="users" size={14} />
        <span>
          Team settings from <code className="ca-ic">.codealong/config.toml</code> on {file.branch} at <code className="ca-ic">{file.commit.slice(0, 7)}</code>
        </span>
        <Button size="sm" onClick={() => void openTeamFile(repo).catch((e: unknown) => setProblem(errorText(e)))}>
          Open file
        </Button>
      </div>
      {file.error && (
        <p className="ca-form-error" role="alert">
          They aren’t used: {file.error}
        </p>
      )}
      {file.skipped.length > 0 && <p className="ca-settings-team-note">Passed over, as this Codealong doesn’t know them: {file.skipped.join(', ')}.</p>}
      {file.settings && file.settings.neverSubmit.length > 0 && (
        <p className="ca-settings-team-note">The team never has {file.settings.neverSubmit.map((v) => VERDICT_WORDS[v]).join(' or ')} submitted for anyone, whatever Submit the agent’s review for you says.</p>
      )}
      {offers.length > 0 && (
        <div className="ca-settings-team-offer">
          <p>The team suggests:</p>
          <ul>
            {offers.map((w) => (
              <li key={w}>{w.charAt(0).toUpperCase() + w.slice(1)}</li>
            ))}
          </ul>
          <Button variant="primary" size="sm" disabled={disabled || taking} onClick={take}>
            Use them
          </Button>
        </div>
      )}
      {file.settings && offers.length === 0 && <p className="ca-settings-team-note">You have what the team suggests turning on, if anything. A change to the file goes for the team once it’s merged into {file.branch}.</p>}
      {problem && (
        <p className="ca-form-error" role="alert">
          {problem}
        </p>
      )}
    </section>
  );
}

/** What a repo's team adds after a reviewer's instructions, to read. */
function TeamWords({ repo, words }: { repo: string; words: string }) {
  if (!words.trim()) return null;
  return (
    <details className="ca-settings-teamwords">
      <summary>{repo}’s team adds this after it</summary>
      <div>{words}</div>
    </details>
  );
}

/** `[reviews]`: an agent reviews the pull requests that wait on your review, on the account and
 *  model picked, told what to do in the box under them (AgentPromptBox.tsx; Codealong's own
 *  instructions until you change them: `defaultPrompt`), its review put where you say
 *  (autoreview.rs). Only accounts that can start a review where `start` says are offered, and
 *  where reviews start goes for every repo, so it shows only without one picked. `reviews` is as
 *  it goes everywhere, or for the repo `scope` names, and `set` changes it there. */
function ReviewsField({ reviews, set, setOwn, scope, team, disabled, defaultPrompt, usage, now }: { reviews: Reviews; set: (next: Partial<Omit<ReviewsCfg, 'own'>>) => void; setOwn: (next: Partial<OwnReviewsCfg>) => void; scope?: Scope; team?: TeamCfg; disabled: boolean; defaultPrompt: string; usage: Usage[]; now: number }) {
  const [all, setAll] = useState<StartAccount[]>([]);
  useEffect(() => {
    // Without them the box says there are no accounts, and config.toml's choice stays as it is.
    startOptions()
      .then((o) => setAll(o.accounts))
      .catch(() => {});
  }, []);
  const start = REVIEW_STARTS.find((s) => s.id === reviews.start) ?? REVIEW_STARTS[0];
  const accounts = all.filter((a) => reviewsRunIn(a, start.id));
  const post = REVIEW_POSTS.find((p) => p.id === reviews.post) ?? REVIEW_POSTS[0];
  const submit = REVIEW_SUBMITS.find((s) => s.id === reviews.submit) ?? REVIEW_SUBMITS[0];
  return (
    <fieldset className="ca-settings-fields" disabled={disabled}>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={reviews.auto} onChange={(e) => set({ auto: e.target.checked })} />
        <span>
          Review pull requests that wait on your review<SourceTag field="reviews.auto" scope={scope} />
        </span>
        <small>
          An agent reviews each open pull request that asks for your review, in a session of its own, and again when new commits come in while it still waits on you. It reads the pull request and the code around it and changes nothing. A pull request joins what waits on you once its
          review is done, and its card says where that stands. Off, an agent reviews one when you ask, with Review with… on the pull request: the same review, by the reviewer below, its review going as below.
        </small>
      </label>
      <div className="ca-field">
        <span>
          The reviewer<SourceTag field="reviews.reviewer" scope={scope} />
          {scope && scope.source('reviews.prompt') === 'repo' && <SourceTag field="reviews.prompt" scope={scope} />}
        </span>
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
        {scope && <TeamWords repo={scope.repo} words={team?.reviewInstructions ?? ''} />}
        <small>
          What it’s told: what to look for and how. It can start subagents of its own and combine what they find; in Claude Code, ask for the Task tool and name each one’s model. Codealong still tells it which pull request, where it’s written out, to change nothing, and how to lay out
          its last message so the review can be posted. <ReviewerRuns account={reviews.account} accounts={all} start={start} auto="Auto picks an account that can start one where reviews run." /> This is apart from the account new sessions start on.
        </small>
      </div>
      {!scope && (
        <label className="ca-field">
          <span>Where reviews run</span>
          <ProfileSelect value={start.id} profiles={REVIEW_STARTS} onChange={(id) => set({ start: id as ReviewStart })} />
          <small>{start.says} Reviews of your own pull requests, and Review with…, start there too, for every repo.</small>
        </label>
      )}
      <label className="ca-field">
        <span>
          Where each review goes<SourceTag field="reviews.post" scope={scope} />
        </span>
        <ProfileSelect value={reviews.post} profiles={REVIEW_POSTS} onChange={(id) => set({ post: id as ReviewPost })} />
        <small>{post.says}</small>
      </label>
      <label className="ca-field">
        <span>
          Submit the agent’s review for you<SourceTag field="reviews.submit" scope={scope} />
        </span>
        <ProfileSelect value={reviews.submit} profiles={REVIEW_SUBMITS} onChange={(id) => set({ submit: id as ReviewSubmit })} />
        <small>
          {submit.says}
          {scope && team && team.neverSubmit.length > 0 && ` In ${scope.repo}, its team never has ${team.neverSubmit.map((v) => VERDICT_WORDS[v]).join(' or ')} submitted for anyone.`}
        </small>
      </label>
      <OwnReviewsField own={reviews.own} setOwn={setOwn} scope={scope} team={team} accounts={accounts} all={all} start={start} defaultPrompt={defaultPrompt} usage={usage} now={now} disabled={disabled} />
    </fieldset>
  );
}

/** Where the reviewer named runs (`reviewsRunIn`), or that it can't start a review where reviews
 *  run, which has its reviews fail until one or the other changes; `auto` without one named. */
function ReviewerRuns({ account, accounts, start, auto }: { account: string; accounts: StartAccount[]; start: (typeof REVIEW_STARTS)[number]; auto: string }) {
  const named = accounts.find((a) => a.name === account);
  if (!named) return <>{auto}</>;
  const runs = reviewsRunIn(named, start.id);
  return <>{runs ? `It runs ${runs}.` : `${named.name} can’t start a review ${start.where}: pick another reviewer, or change where reviews run.`}</>;
}

/** `[reviews.own]`: the pull requests your sessions open, each reviewed by an agent on another
 *  model or provider than the session that opened it, its review kept here and handed back to
 *  that session (autoreview.rs). The reviewer is the same box as the one above, among the same
 *  accounts (`accounts`, of `all`), and runs where reviews run (`start`). */
function OwnReviewsField({ own, setOwn, scope, team, accounts, all, start, defaultPrompt, usage, now, disabled }: { own: Required<OwnReviewsCfg>; setOwn: (next: Partial<OwnReviewsCfg>) => void; scope?: Scope; team?: TeamCfg; accounts: StartAccount[]; all: StartAccount[]; start: (typeof REVIEW_STARTS)[number]; defaultPrompt: string; usage: Usage[]; now: number; disabled?: boolean }) {
  return (
    <>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={own.auto} onChange={(e) => setOwn({ auto: e.target.checked })} />
        <span>
          Review the pull requests your sessions open<SourceTag field="own.auto" scope={scope} />
        </span>
        <small>
          An agent on another model reviews each open pull request a session of yours opened here, in a session of its own, and again when new commits come in. It reads the pull request and the code around it and changes nothing. Its review stays in Codealong, in the pull request’s Review
          tab: nothing goes on GitHub.
        </small>
      </label>
      {own.auto && (
        <>
          <div className="ca-field">
            <span>
              Reviewer of your pull requests<SourceTag field="own.reviewer" scope={scope} />
              {scope && scope.source('own.prompt') === 'repo' && <SourceTag field="own.prompt" scope={scope} />}
            </span>
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
              onAgent={(account, model) => setOwn({ account, model })}
              onPrompt={(prompt) => setOwn({ prompt })}
            />
            {scope && <TeamWords repo={scope.repo} words={team?.ownReviewInstructions ?? ''} />}
            <small>
              <ReviewerRuns account={own.account} accounts={all} start={start} auto="Auto picks an account of another harness than the session that opened each pull request, among those that can start one where reviews run: Codex reviews what Claude Code wrote, say." /> Pick one with its model to have that review every one.
            </small>
          </div>
          <label className="ca-field ca-settings-check">
            <input type="checkbox" checked={own.send} onChange={(e) => setOwn({ send: e.target.checked })} />
            <span>
              Hand each review to the session that opened the pull request<SourceTag field="own.send" scope={scope} />
            </span>
            <small>
              Once that session runs here and doesn’t wait on you, it’s sent a message naming the review, kept as a file it reads, to fix what holds up and say what it leaves. A review that finds nothing to raise isn’t sent, nor one of commits it has moved on from. Off, send one from the pull
              request’s Reviews tab.
            </small>
          </label>
          <label className="ca-field ca-settings-check">
            <input type="checkbox" checked={own.drafts} onChange={(e) => setOwn({ drafts: e.target.checked })} />
            <span>
              Review your draft pull requests too<SourceTag field="own.drafts" scope={scope} />
            </span>
            <small>Off, a pull request of yours is reviewed once it’s ready for review. Drafts of other people’s are never reviewed.</small>
          </label>
        </>
      )}
    </>
  );
}

/** `[pull_requests] nudge_by_default`: every open PR of yours tells the session that opened it
 *  what happens on it, unless its own switch is off (nudges.rs). `prs` is as it goes everywhere,
 *  or for the repo `scope` names. */
function NudgeField({ prs, set, scope, disabled }: { prs: Required<PullRequestsCfg>; set: (next: Partial<PullRequestsCfg>) => void; scope?: Scope; disabled: boolean }) {
  const about = NUDGE_ABOUT.find((n) => n.id === prs.nudge_about) ?? NUDGE_ABOUT[0];
  return (
    <fieldset className="ca-settings-fields" disabled={disabled}>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={prs.nudge_by_default} onChange={(e) => set({ nudge_by_default: e.target.checked })} />
        <span>
          Nudge the session that opened a pull request<SourceTag field="prs.nudge_by_default" scope={scope} />
        </span>
        <small>
          For every open pull request of yours, the session that opened it is sent a message, as one of yours, when something happens on it (below), so it takes it from there. The message never asks it to merge: it merges only if you asked it to. Its other sessions aren’t told. A pull
          request’s own switch, on its card, wins over this. Off, a pull request nudges nobody until you turn it on there, or an agent does with <code>codealong pr nudge</code>.
        </small>
      </label>
      <label className="ca-field">
        <span>
          What a nudge tells it about<SourceTag field="prs.nudge_about" scope={scope} />
        </span>
        <ProfileSelect value={prs.nudge_about} profiles={NUDGE_ABOUT} onChange={(id) => set({ nudge_about: id as NudgeAbout })} />
        <small>{about.says} This holds for every pull request with its nudge on, by this switch or its own.</small>
      </label>
    </fieldset>
  );
}

/** What a nudge tells of (config.rs `NudgeAbout`, nudges.rs `keep`), as the select offers it. */
const NUDGE_ABOUT: (Profile & { id: NudgeAbout; says: string })[] = [
  { id: 'everything', name: 'Everything that happens on it', says: 'When someone else reviews it, asks for changes or comments, when it’s approved, merged or closed, and when its checks fail.' },
  { id: 'feedback', name: 'Only comments and requests for changes', says: 'Only when someone else comments on it, asks for changes, or reviews it with something to say, so it can work their feedback in. Never for an approval, a merge or a close, nor for failing checks.' },
  { id: 'feedback-and-checks', name: 'Comments, requests for changes and failing checks', says: 'When someone else comments on it, asks for changes, or reviews it with something to say, and when its checks fail: what asks for work. Never for an approval, a merge or a close.' },
];

/** config.rs's DEFAULT_QUICK_PANEL. */
const DEFAULT_QUICK_PANEL = 'Option+Cmd+/';
/** config.rs's DEFAULT_WAITING. */
const DEFAULT_WAITING = 'Option+Cmd+.';
const DEFAULT_IMPROVE = 'Option+Cmd+,';

/** The board goes back this far, so searching further finds nothing more (config.rs). */
const MAX_SEARCH_DAYS = 14;

/** The choices for how far back text search reaches, with a value typed in config.toml too. */
function dayOptions(current: number): Profile[] {
  const days = [...new Set([0, 1, 3, 7, MAX_SEARCH_DAYS, current])].sort((a, b) => a - b);
  return days.map((d) => ({ id: String(d), name: d === 0 ? 'Off: titles only' : d === 1 ? 'The last day' : `The last ${d} days` }));
}

const JEV_OFF: JevCfg = { model: 'jev-latest', quick_panel: false, sessions: false, todos: false, pull_requests: false, emblems: false };

type JevFeature = 'quick_panel' | 'sessions' | 'todos' | 'pull_requests' | 'emblems';

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
  {
    id: 'emblems',
    label: 'Session emblems',
    does: 'Puts a small icon beside each session’s title, for what it works on, so you can tell sessions apart at a glance and remember one as “the teal bug”. Its background shades between two colors that go together, lit from one of eight places, which Codealong picks so the sessions beside it look different. The color you give a session with Claude Code’s /color is one of the two.',
    sends: 'Each new session’s title and first prompt, once. The icon is kept for the session’s life.',
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
  /** The other shortcuts, which this one can't be too. */
  others: { value: string; name: string }[];
  onChange: (shortcut: string) => void;
}

/**
 * One of the shortcuts that work in every app: shown the way macOS writes shortcuts, and recorded
 * by pressing it. While it records, the shortcuts are off, so pressing one records it instead of
 * doing what it does. Engine-side problems (a shortcut macOS won't take) show under it, and so
 * does what else on a Mac is known to use the same keys.
 */
function ShortcutField({ which, name, fallback, value, others, onChange }: ShortcutFieldProps) {
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
            const same = others.find((o) => shortcutLabel(made.shortcut) === shortcutLabel(o.value));
            if (same) {
              setProblem(`${shortcutLabel(made.shortcut)} is ${same.name}. Pick another.`);
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
            {value !== fallback && others.every((o) => shortcutLabel(fallback) !== shortcutLabel(o.value)) && (
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

/** Settings' `focus` in Projects that opens a new project's form. */
export const NEW_PROJECT = 'new';

interface ProjectDraft {
  /** The project being edited, or null when adding one. */
  original: string | null;
  name: string;
  repos: string[];
  folders: string[];
  /** '' opens its previews in the default browser. */
  chromeProfile: string;
}

const NO_DRAFT: ProjectDraft = { original: null, name: '', repos: [], folders: [], chromeProfile: '' };

function ProjectsPane({ cfg, chromeProfiles, disabled, onSave, lanes, adding }: PaneProps & { chromeProfiles: ChromeProfile[]; lanes: Lane[]; adding: boolean }) {
  const [draft, setDraft] = useState<ProjectDraft | null>(adding ? NO_DRAFT : null);
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
                setDraft(NO_DRAFT);
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
