// The board as the Rust engine serializes it (src-tauri/src/model.rs). Option<T> arrives as null.
//
// The types are in src/types/, a file a subject, and this file re-exports them all: everything
// imports from './types', never from a file under it. A new type goes in the file whose subject
// it is, beside what it's used with; a subject none of them covers gets a file of its own there
// and a line here. One that several subjects use goes where the engine defines it, and the
// others `import type` it. src-tauri/src/wiretypes.rs holds the interfaces to the engine's model
// by name (src/testing/modelFields.ts), whichever file they are in.
//
// A file that holds only types is re-exported with `export type *`, so importing from here loads
// no module for it at run time. One that holds a constant too needs `export *`: change its line
// when a file gets its first.

// A session and what hangs off it: its status, what its last turn ended on, its mode, how its turn was read.
export type * from './types/session';
// A worktree (a lane): its git, the ways to open it, its preview, the device it is on.
export type * from './types/lane';
// Pull requests: a PR on a worktree and in the PRs view, its detail, reviews of it, its merge box.
export type * from './types/pr';
// The board itself: everything the engine publishes at once, its projects, notices and load.
export type * from './types/board';
// config.toml, table by table, a repo's team file, and what Settings works from.
export * from './types/config';
// An account and what is read of it: plan usage and limits, resets, training, sign-in, MCP servers, setup.
export type * from './types/accounts';
// What model calls used and cost, and your part in the work: the Analytics window, Settings › Decisions.
export type * from './types/analytics';
// Experiment bake-off runs and leaderboard: the Analytics window's Experiments tab.
export type * from './types/experiments';
// A session's conversation: transcript pages, what it waits on you to allow, what a send did, search.
export type * from './types/conversation';
// Side conversations about a session or a PR.
export type * from './types/side';
// Starting a session: what the quick panel and the new-session sheet offer, Jev's suggestions.
export type * from './types/start';
// Skills a session can run by name, and the command, skill and plugin each account has installed.
export type * from './types/skills';
// Todos, the labels they wear, and kept attachments.
export * from './types/todos';
// Schedules: when one runs, what it does, its runs, and the ones the harnesses keep.
export type * from './types/schedules';
// Your other devices: this one, the linked ones, the ones found or asking to link.
export type * from './types/devices';
// Voice: meetings from your calendar, sessions that listen, the keys held to talk.
export type * from './types/voice';
// Codealong's own terminals: one as the board lists it, its screen, what a window sends it.
export type * from './types/terminals';
// What Close idle sessions and Close stale terminals would close, and what closing did.
export type * from './types/closing';
// What the windows without the board are sent: the bar's and the notch's feed, a desktop pin's, the peek.
export type * from './types/feeds';
// Codealong itself on this Mac: updates, opening at login, its shortcuts, improving it, editors.
export type * from './types/app';
