// config.toml as the engine parses it, table by table, and what Settings works from. Mirrors
// src-tauri/src/config.rs; a repo's team file is teamfile.rs's. Keys are the files' own.

import type { Rank } from './devices';
import type { Verdict } from './pr';
import type { Harness } from './session';
import type { StartPlace } from './start';

/** `[sessions]` (config.rs `SessionsCfg`). */
export interface SessionsCfg {
  /** Block automatic messages, resumes and moves after a provider flag. Defaults to true. */
  block_flagged_sessions?: boolean;
  continue_after_network_error: boolean;
  /** A session whose account's sign-in was turned down is told to carry on once the account reads
   *  signed in again (reconnect.rs). Files from before it have none: on. */
  continue_after_sign_in?: boolean;
  /** A session Codealong runs, cut off by Codealong quitting, is told to carry on as it comes back
   *  (reconnect.rs). Files from before it have none: on. */
  continue_after_restart?: boolean;
  /** A session that isn't running is resumed out of sight when one it started, or the one that
   *  started it, sends it a message (send.rs `wakes`). Files from before it have none: on. */
  resume_for_sessions_it_started?: boolean;
  /** The same for any session's message, the one above being part of it while it's on (send.rs
   *  `wakes`). Files from before it have none: on. */
  resume_for_any_session?: boolean;
  /** A PR's primary session that isn't running is resumed out of sight to be told what asks for
   *  work on the PR, unless another of its sessions runs (nudges.rs `stopped_primaries`). Files from
   *  before it have none: on. */
  resume_for_pr_news?: boolean;
  /** Claude Code sessions hand Codealong a reply as their terminal shows it (the plugin's
   *  `MessageDisplay` hook), so the panel shows it as it's written; their terminals then show a
   *  reply a paragraph at a time. New sessions only. Files from before it have none: off. */
  live_replies?: boolean;
  /** Minutes a session in a terminal, or a background job, sits idle, wanting nothing from you,
   *  before its tab closes or its job stops to free what its agent holds; a click or a reply
   *  resumes it (hibernate.rs). 0, or none, is off. */
  hibernate_after?: number;
}

/** An `[[accounts]]` table in config.toml. */
export interface AccountCfg {
  name: string;
  harness: Harness;
  home: string;
  command: string | null;
  /** Where a new session starts unless you pick another; absent leaves it to the usual place. */
  start?: StartPlace | null;
  /** The command that runs its Agent Client Protocol adapter. */
  acp?: string | null;
  /** What its plan costs you a month, in dollars: what the Analytics window counts as paid. */
  monthly?: number | null;
  /** It pays for each token at API prices (an API key). */
  per_token?: boolean;
  /** The usage meter percentage at or above which Auto and routing groups pass over this account
   *  (1 to 100; 90 when omitted). */
  usage_cap?: number | null;
  /** Turned off: Codealong starts nothing on it and leaves it out of the usage bar; its sessions
   *  still show. */
  disabled?: boolean;
  /** The Chrome profile signed in to it, where the pages its sessions make open (by its name, the
   *  account signed in to it or its folder); absent opens them in the default browser. */
  chrome_profile?: string | null;
}

/** How a routing group picks among its members with room (config.rs `RoutingStrategy`). */
export type RoutingStrategy = 'usage' | 'alternate-harness' | 'round-robin' | 'fallback';

/** One member of a `[[routing.groups]]` table (config.rs `GroupMemberCfg`): an account, on a
 *  model (empty for Auto's) at an effort (empty for the account's own). */
export interface GroupMemberCfg {
  account: string;
  model?: string;
  effort?: string;
}

/** A `[[routing.groups]]` table in config.toml (config.rs `RoutingGroupCfg`). */
export interface RoutingGroupCfg {
  name: string;
  strategy: RoutingStrategy;
  members: GroupMemberCfg[];
  /** A boundary (boundary.rs): only members on a commercial sign-in count as in it, and what
   *  its members' sessions say stays with sessions in it. */
  boundary?: boolean;
  /** For a boundary, the decision model that may read its sessions: a `[[decisions.models]]`
   *  name, or `none`. Missing is Jev, whatever `[decisions] default` says. */
  decisions?: string | null;
}

/** `[routing]` in config.toml (config.rs `RoutingCfg`). */
export interface RoutingCfg {
  /** The routing group `Auto` uses when starting sessions; empty uses the first account's harness by usage room. */
  auto?: string;
  /** Optional guidance for Jev when suggesting an agent or model in the quick panel. */
  prompt?: string;
  /** The room (percent, 0 to 50) below each account's `usage_cap` left for the sessions already
   *  running on it; absent is 5. */
  buffer?: number | null;
  groups?: RoutingGroupCfg[];
}

/** A `[[projects]]` table in config.toml. */
export interface ProjectCfg {
  name: string;
  /** `github.com/owner/*` or `github.com/owner/name`. */
  repos: string[];
  /** Other folders that belong to the project; a repo among them is this project's, whatever its remote matches. */
  folders: string[];
  /** The Chrome profile its worktrees' previews open in; missing opens them in the default
   *  browser. */
  chrome_profile?: string;
  /** The routing group a start here on Auto goes on, over `[routing] auto`; missing for
   *  `[routing] auto`'s. */
  default?: string;
  /** The decision model that may read what's said here: a `[[decisions.models]]` name, or
   *  `none`; missing for `[decisions] default`'s (decisions.rs `for_project`). */
  decisions?: string;
}

/** config.toml, as the engine parses it (src-tauri/src/config.rs). Keys are the file's own. */
export interface Config {
  scan: string[];
  apps: { terminal: string; editor: string; new_terminal?: NewTerminal; move_pointer?: boolean; close_command_tabs_after?: number; tmux?: TmuxUse; run_in?: 'codealong' | 'terminal' };
  /** `zoom` is a percent; Settings leaves it as config.toml has it. */
  /** `glass`: the board's windows show what's behind them through macOS's glass (glass.rs); on by
   *  default on macOS, and the engine sends what it comes to. `show_keys`: keyboard shortcuts show
   *  all the time, not only while ⌘ or ⌃ is held (keysShown.ts). `tools`: how tool calls appear
   *  in conversations (toolDisplay.ts), folded by default. */
  appearance: { theme: Theme; zoom?: number; glass?: boolean; show_keys?: boolean; tools?: ToolDisplay };
  /** Shortcuts that work in any app, as config.toml writes them (`Option+Cmd+/`); empty is off. */
  shortcuts: { quick_panel: string; chat?: string; waiting: string; improve?: string; main_window?: string; talk?: string; talk_orchestrator?: string; listen?: string };
  /** What Codealong asks decision models, feature by feature, and the models. Older files have
   *  none (or `[jev]`, which the engine reads as this): all off, Jev the only model. */
  decisions?: DecisionsCfg;
  /** Session emblems. Older files have none: off. */
  emblems?: EmblemsCfg;
  accounts: AccountCfg[];
  projects: ProjectCfg[];
  /** How many days of sessions have what was said in them searched; 0 is off. */
  search?: { days: number };
  /** Your other Macs. Older files have none: off. */
  devices?: DevicesCfg;
  /** Automatic updates. Older files have none: on. */
  updates?: { check: boolean };
  /** What's done about a session a usage limit stopped. Older files have none: suggest, and
   *  carry on once it lifts. */
  limits?: { when_hit: WhenLimited; continue_when_lifted?: boolean };
  /** Routing groups and default routing rules. Older files have none. */
  routing?: RoutingCfg;
  /** What Codealong does for sessions by itself. Older files have none: carry on after a lost
   *  connection, and resume a session for any session's message. */
  sessions?: SessionsCfg;
  /** Reviewing the PRs that wait on your review without being asked. Older files have none: off. */
  reviews?: ReviewsCfg;
  /** Your PRs. Older files have none: no nudges until turned on PR by PR. */
  pull_requests?: PullRequestsCfg;
  /** Your settings for particular repos, `[repos."github.com/owner/name"]`, by that key
   *  (repoSettings.ts `repoKey`). Older files have none. */
  repos?: Record<string, RepoCfg>;
  /** What waits on you, and in what order. Older engines send none: the default order. */
  queue?: QueueCfg;
  /** A sound when something starts to wait on you. Older files have none: Pop, Tink, none, 60. */
  sounds?: SoundsCfg;
  /** Features still being tried out. Older files have none: all off. */
  experimental?: ExperimentalCfg;
  /** Voice's settings, which matter once `experimental.voice` is on. Older files have none. */
  voice?: VoiceCfg;
  debug: { perf_log: boolean };
  /** `[prices."<model>"]`: API prices for models Codealong has none or the wrong ones for
   *  (prices.rs `Price`). Settings leaves them as config.toml has them. */
  prices?: Record<string, unknown>;
}

/** `[experimental]` (config.rs `ExperimentalCfg`): features still being tried out, each off until
 *  it's turned on. `boards`: the Board view, as Kanban and as Canvas, and Add to board. `voice`:
 *  the keys held to talk, and sessions that listen. `analytics`: who began each session and turn,
 *  what your prompts did, and how the sessions you marked done stood (the Analytics window's Your part
 *  and Sessions). Missing from files and boards before it: off. */
export interface ExperimentalCfg {
  boards: boolean;
  voice?: boolean;
  analytics?: boolean;
}

/** `[voice]` (config.rs `VoiceCfg`). */
export interface VoiceCfg {
  /** The language spoken, as a locale ("en-US"); empty for the Mac's own. */
  locale: string;
  /** What's said to the orchestrator is sent as the key comes up, not put in its box first. */
  send_on_release: boolean;
  /** How often a session that listens is told what's new, at most, in seconds. */
  tell_every: number;
  /** A meeting in your calendar is offered as it's about to start, to take its notes
   *  (meetings.rs). Off until you turn it on. Missing from older engines, as the rest are. */
  meetings?: boolean;
  /** Where your meetings are kept, a folder each; empty for `meetings` beside config.toml. */
  meetings_folder?: string;
  /** The account a meeting's session starts on (an account, a routing group, or empty for
   *  Auto), and its model, empty for the account's own. */
  meeting_account?: string;
  meeting_model?: string;
  /** How many minutes before a meeting starts it's offered. */
  meeting_lead?: number;
  meeting_countdown?: number;
  /** The calendars whose meetings aren't offered, by the ids macOS has for them. */
  meeting_calendars_off?: string[];
}

/** `[pull_requests]` (config.rs `PullRequestsCfg`). */
export interface PullRequestsCfg {
  nudge_by_default: boolean;
  nudge_about?: NudgeAbout;
  /** Run a read-only side-fork triage helper before notifying/waking a PR's primary session.
   *  Older files have none: on. */
  triage_nudges?: boolean;
  /** Bot logins allowed to trigger PR nudges (empty ignores all bots; `"*"` allows all). */
  nudge_bots?: string[];
  /** Ask a reviewer who asked for changes to review again once commits came and nothing was said
   *  (rerequest.rs). Missing from older engines. */
  rerequest_after_fixes?: boolean;
  /** PRs waiting on your review go ahead of the sessions that need you in what waits on you
   *  (queue.ts `byPlace`). Missing from older engines. */
  reviews_first?: boolean;
}

/** Your settings for one repo's pull requests (config.rs `RepoCfg`): each value set goes for that
 *  repo over `[reviews]`'s or `[pull_requests]`'s and over the repo's team's file; what's left out
 *  is as they say. */
export interface RepoCfg {
  reviews?: ReviewsPatch;
  pull_requests?: Partial<PullRequestsCfg>;
}

/** `[reviews]` for one repo, key by key. */
export interface ReviewsPatch {
  auto?: boolean;
  account?: string;
  model?: string;
  effort?: string;
  other_harness?: boolean;
  drafts?: boolean;
  prompt?: string;
  post?: ReviewPost;
  submit?: ReviewSubmit;
  verify?: boolean;
  verify_account?: string;
  verify_model?: string;
  own?: Partial<OwnReviewsCfg>;
}

/** What a repo's `.codealong/config.toml` sets for its team (teamfile.rs `TeamCfg`). */
export interface TeamCfg {
  /** Added to what the reviewer of a PR that waits on your review is told. */
  reviewInstructions: string;
  /** Added to what the reviewer of a PR of yours is told. */
  ownReviewInstructions: string;
  /** Verdicts never submitted as anyone's review there. */
  neverSubmit: Verdict[];
  /** What a nudge tells about, for everyone. */
  nudgeAbout?: NudgeAbout | null;
  /** Its PRs waiting on your review go ahead of the sessions that need you, for everyone. */
  reviewsFirst?: boolean | null;
  /** What the team suggests each person turns on. */
  suggests: RepoCfg;
}

/** A repo's team file on one branch, as read from that branch as fetched (teamfile.rs
 *  `TeamFile`). */
export interface TeamFile {
  /** `owner/name`. */
  repo: string;
  branch: string;
  /** It's the repo's default branch, which Settings shows. */
  default: boolean;
  /** The commit it was read at. */
  commit: string;
  /** What it sets; none when it goes unused. */
  settings?: TeamCfg | null;
  /** Why it goes unused. */
  error?: string | null;
  /** Keys it has that this Codealong passed over. */
  skipped: string[];
  /** What it suggests that isn't how things go for you in its repo now: what Use them changes. */
  offers: RepoCfg;
}

/** `[queue]` (config.rs `QueueCfg`): what waits on you, in the order the queue goes to it
 *  (queue.ts `WAITS` names the kinds, `queueKinds` reads it), and how many days before its due
 *  date a todo comes due. */
export interface QueueCfg {
  order: string[];
  due_within: number;
}

/** `[sounds]` (config.rs `SoundsCfg`, sounds.rs): each the name of a sound in ~/Library/Sounds or
 *  macOS's, or '' for none. */
export interface SoundsCfg {
  needs_you: string;
  your_turn: string;
  review_requested: string;
  /** A meeting in your calendar is about to start (meetings.rs). Missing from older engines. */
  meeting?: string;
  /** In percent of the Mac's own volume. */
  volume: number;
}

/** What a PR whose nudge is on tells its session about (config.rs `NudgeAbout`): everything, or
 *  only what asks for work (comments and requests for changes), with failing checks and conflicts or
 *  without, and with a merge or without (prStatus.ts `nudgeKind`, `nudgeMerges`).
 *  Older files have none: everything. */
export type NudgeAbout = 'everything' | 'feedback' | 'feedback-and-checks' | 'feedback-and-merges' | 'feedback-checks-and-merges';

/** `[reviews]` (config.rs `ReviewsCfg`, autoreview.rs). */
export interface ReviewsCfg {
  /** An agent reviews each open PR that waits on your review, and again when new commits come in. */
  auto: boolean;
  /** The account or routing group it runs on; empty for the one Auto would start. */
  account?: string;
  /** The model it starts on; empty for the account's own. */
  model?: string;
  /** The effort it starts at, with an account picked; empty for the account's own (a routing
   *  group's members name theirs). */
  effort?: string;
  /** Keep off the harness that wrote the PR (from a session here or a commit/PR trailer) when
   *  another harness in the pool can review. Older files have none: on. */
  other_harness?: boolean;
  /** Review someone else's draft too when it asks for your review. Older files have none: on.
   *  Being assigned to a draft doesn't ask for your review. */
  drafts?: boolean;
  /** What it's told to do; empty for Codealong's own instructions (`ConfigView.reviewPrompt`). */
  prompt?: string;
  /** Where review sessions start, of your own PRs too. Older files have none: never in a
   *  terminal. */
  start?: ReviewStart;
  /** Where each review goes. Older files have none: a draft. */
  post?: ReviewPost;
  /** Which of the agent's verdicts goes as your review once it's posted. Older files have none:
   *  never. */
  submit?: ReviewSubmit;
  /** Verify findings with a critic pass before posting or handing back. Older files have none: on. */
  verify?: boolean;
  /** Account or routing group for the verifier pass; empty picks one automatically. */
  verify_account?: string;
  /** Model for the verifier pass; empty uses the verifier account's own. */
  verify_model?: string;
  /** The pull requests your sessions open (`[reviews.own]`). */
  own?: OwnReviewsCfg;
}

/** Reviews of the PRs your sessions open (config.rs `OwnReviewsCfg`): on another model or
 *  provider than each one's primary session, kept here and handed back to that session. */
export interface OwnReviewsCfg {
  auto: boolean;
  /** The account or routing group they run on; empty for the one Auto would start among those of
   *  another harness than the PR's session. */
  account?: string;
  model?: string;
  effort?: string;
  /** Keep off the harness that opened the PR when another harness in the pool can review. Older
   *  files have none: on. */
  other_harness?: boolean;
  /** What the reviewer is told to do; empty for Codealong's own instructions. */
  prompt?: string;
  /** Hand each review to the PR's primary session. Older files have none: on. */
  send?: boolean;
  /** Hand back `Verdict: comment` reviews automatically too. Older files have none: off. */
  send_comments?: boolean;
  /** Maximum automatic own-PR review rounds per PR (0 = unlimited). Older files have none: 2. */
  max_rounds?: number;
  /** Review your draft PRs too. Older files have none: off. Others' drafts go by `ReviewsCfg.drafts`. */
  drafts?: boolean;
}

/** Where an automatic review goes (config.rs `ReviewPost`): a draft review on your GitHub account
 *  for you to finish, a comment on the PR, or nowhere but its sessions. */
export type ReviewPost = 'draft' | 'comment' | 'none';

/** Where a review's session starts (config.rs `ReviewStart`, autoreview.rs `review_places`):
 *  never in a terminal (the harness's background service, else Codealong), in Codealong, as the
 *  account's Background goes (tmux with no window for a harness with neither), or in
 *  your terminal, in front. */
export type ReviewStart = 'no-terminal' | 'codealong' | 'background' | 'terminal';

/** Which of an automatic review's verdicts are submitted as your review (config.rs
 *  `ReviewSubmit`): none, approvals, approvals and requests for changes, or approvals with those
 *  requests sent as comments; or the requests alone, as such or as comments, approvals left to
 *  you. */
export type ReviewSubmit = 'never' | 'approve' | 'approve-or-request-changes' | 'approve-or-comment' | 'request-changes' | 'comment';

/** `[devices]` in config.toml: Codealong on your other devices (src-tauri/src/peers.rs). */
export interface DevicesCfg {
  enabled: boolean;
  /** This device's name on the others; empty is the computer's name. */
  name: string;
  /** Find your devices on your tailnet. */
  tailscale: boolean;
  /** Reach linked devices anywhere, through iroh. */
  iroh: boolean;
  /** Let linked devices send prompts, answer approvals and start sessions here. */
  remote_control: boolean;
  /** Show their sessions in the bar, the notch and Waiting on you too, while they're connected. */
  in_bar: boolean;
  port: number;
  /** Where this machine stands for the automations your machines share (shared.rs). */
  automations: Rank;
}

export const DEVICES_OFF: DevicesCfg = { enabled: false, name: '', tailscale: true, iroh: true, remote_control: false, in_bar: true, port: 47471, automations: 'first' };

/** Where a terminal Codealong opens goes: its own window, a tab in the front window, or a tab
 *  beside the same worktree, repo or project. */
export type NewTerminal = 'window' | 'tab' | 'project';

/** When sessions Codealong starts run in tmux, shown in the terminal (`[apps] tmux`, config.rs
 *  `TmuxUse`). */
export type TmuxUse = 'always' | 'when-locked' | 'never';

/** A terminal or editor profile `[apps]` can name. */
export interface Profile {
  id: string;
  name: string;
}

/** What Settings works from: the settings as parsed, the file as written, and its error. */
export interface ConfigView {
  config: Config;
  text: string;
  path: string;
  /** Why the file can't be parsed; then `config` is the defaults and the form is read-only. */
  error: string | null;
  terminals: Profile[];
  editors: Profile[];
  /** Chrome's profiles, in its profile picker's order; empty when Chrome isn't set up here. */
  chromeProfiles: ChromeProfile[];
  /** What a reviewer is told to do while `[reviews] prompt` is empty (autoreview.rs
   *  `INSTRUCTIONS`). Missing from older engines. */
  reviewPrompt?: string;
  /** The sounds `[sounds]` can name, sorted (sounds.rs `names`). Missing from older engines. */
  sounds?: string[];
  /** The shortcuts a new config.toml has on this OS (config.rs `default_shortcuts`), which
   *  Settings offers to put back. Missing from older engines. */
  defaultShortcuts?: Config['shortcuts'];
  /** Whether tmux is where Codealong looks for it, for `[apps] tmux`. Missing from older engines. */
  tmuxInstalled?: boolean;
  /** The terminal profiles that keep sessions out of sight and start them while the screen is
   *  locked by themselves (Rex), for which `[apps] tmux` matters only when it says always.
   *  Missing from older engines. */
  holdsSessions?: string[];
}

/** A Chrome profile (src-tauri/src/chrome.rs). */
export interface ChromeProfile {
  /** What config.toml names it by: its name, or its folder when another profile has that name. */
  id: string;
  name: string;
  /** Its folder in Chrome's data folder: `Default`, `Profile 12`. */
  dir: string;
  /** The Google account signed in to it. */
  email: string | null;
}

/** `suggest` offers to move a session a usage limit stopped; `move` moves it by itself. */
export type WhenLimited = 'suggest' | 'move';

/** `[decisions]` in config.toml (config.rs `DecisionsCfg`): the default model, each feature's
 *  switch, and the models. */
export interface DecisionsCfg {
  /** The model whatever names none goes to: a `models` name (`jev` is always one), or `none`. */
  default: string;
  quick_panel: boolean;
  sessions: boolean;
  todos: boolean;
  pull_requests: boolean;
  /** Reads what each prompt you typed did with what its agent had said, once `[experimental]
   *  analytics` is on. */
  prompts: boolean;
  /** `[[decisions.models]]`; Jev is there without one (decisions.ts `modelsOf`). Missing when
   *  there are none. */
  models?: DecisionModelCfg[];
}

/** A `[[decisions.models]]` table (config.rs `DecisionModelCfg`). */
export interface DecisionModelCfg {
  name: string;
  /** `typesafe`, `openai` or `gemini` (decisions.ts `PROVIDERS`). */
  provider: string;
  model: string;
  /** The model a request goes on to when this one fails for now, for what goes by the default
   *  only. */
  fallback?: string;
}

/** What each session gets beside its title (`[emblems] kind`): nothing, an emoji Jev picks, or a
 *  picture an account draws. */
export type EmblemKind = 'off' | 'emoji' | 'drawn';

/** `[emblems]` in config.toml (config.rs `EmblemsCfg`). */
export interface EmblemsCfg {
  kind: EmblemKind;
  /** The account that draws them, for `drawn`; empty is the first that draws (harness.ts
   *  `DRAWS`). */
  account: string;
  /** How they're drawn, for `drawn`; empty is Codealong's own (SettingsDialog.tsx
   *  `DRAWN_STYLE`). */
  prompt: string;
}

/** Light, dark, or following macOS. */
export type Theme = 'system' | 'light' | 'dark';

/** How tool calls appear in a session's conversation (`[appearance] tools`, src/toolDisplay.ts). */
export type ToolDisplay = 'folded' | 'compact' | 'detailed' | 'zen';
