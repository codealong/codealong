// Codealong itself on this Mac: its updates (updates.rs), opening at login (os/login), its
// shortcuts (quick.rs), where improving it starts (improve.rs), the editors installed
// (editors.rs), and where the page has the pointer (pointer.rs).

/** Where a session that improves Codealong goes (the engine's improve.rs): its project and
 *  worktree, or its source in none of your projects yet, or neither when it isn't on this Mac. */
export interface Improve {
  project?: string | null;
  laneId?: string | null;
  source?: ImproveSource | null;
  /** This app is the installed Codealong (a release), which no change to the source reaches, so
   *  the panel suggests Codealong Dev first (agentTasks.ts `devSetupTask`). Missing otherwise. */
  installed?: boolean;
}

/** A checkout of Codealong's source in none of your projects. */
export interface ImproveSource {
  /** Its main checkout's folder. */
  root: string;
  /** `github.com/owner/name`, as `[[projects]] repos` name repos; missing when it has no remote. */
  repo?: string | null;
}

/** Where automatic updates stand (src-tauri/src/updates.rs): this copy's version, and a state. */
/** What origin/main has that a development build hasn't (updates.rs `Behind`). */
export interface DevBehind {
  /** The commit the build was built from. */
  built: string;
  commits: number;
  /** The checkout of Codealong's source it was looked at in. */
  source: string;
  /** Its worktree, where the session that updates it starts; null for a source in none of your
   *  projects, whose session starts in a chat of its own. */
  laneId: string | null;
  checkedAt: number;
}

export type UpdateView = { current: string; behind?: DevBehind } & (
  | { state: 'off' } // a development build, which never updates itself
  | { state: 'idle' }
  | { state: 'checking' }
  | { state: 'upToDate'; checkedAt: number }
  | { state: 'installing'; version: string }
  // In place, starting at the next launch, or with `inPlace` false, downloaded and going in at
  // Restart to update (updates.rs `installs_at_once`).
  | { state: 'ready'; version: string; notes: string | null; inPlace: boolean }
  | { state: 'failed'; error: string; at: number }
);

/** Whether the app opens when you log in (src-tauri/src/os/login): `approve` is registered, with
 *  macOS waiting for you to allow it in System Settings. */
export type AtLogin = 'off' | 'on' | 'approve';

/** An installed editor's app icon (editors.rs): `size` pixels square, straight RGBA, top row
 *  first. */
export interface EditorIcon {
  id: string;
  size: number;
  rgba: number[];
}

/** The quick panel's shortcut, and why it isn't working when it isn't (src-tauri/src/quick.rs). */
export interface ShortcutStatus {
  /** The quick panel's, the one that opens it set to a chat, the one that goes through what waits
   *  on you, Improve Codealong's, the one that brings the board's window forward, or one of the
   *  two held to talk. */
  which: 'quick' | 'chat' | 'waiting' | 'improve' | 'main' | 'talk' | 'talkorchestrator' | 'listen';
  /** As config.toml writes it; empty when it's off. */
  shortcut: string;
  error: string | null;
}

/** A list under the pointer, as the page has it (pointer.rs `PageScroller`), in CSS pixels. */
export interface PointerScroller {
  /** Its first class name. */
  name: string;
  top: number;
  /** Its content's height and the box it shows it in. */
  height: number;
  viewW: number;
  viewH: number;
  /** Its last scroll positions, newest last, each with how long ago. */
  recent: string[];
}

/** Where the page has the pointer when it comes to rest (pointer.rs `PageProbe`). */
export interface PointerProbe {
  x: number;
  y: number;
  /** How long ago the pointer event was made, in ms. */
  age: number;
  /** Points per CSS pixel. */
  zoom: number;
  /** What the page finds at the pointer. */
  under: string;
  /** The lists that hold it, innermost first. */
  scrollers: PointerScroller[];
}
