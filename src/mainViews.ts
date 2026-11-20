// The main window's views (App.tsx), what each says while it reads and what its find says it
// finds in, and how recently active the sidebar narrows them to.

const DAY = 24 * 60 * 60 * 1000;

/** The views. Kanban and Canvas are the Board view's two layouts (BoardBar.tsx), each with a
 *  chord of its own; the sidebar lists the Board view once. */
export type View = 'sessions' | 'prs' | 'todo' | 'schedules' | 'kanban' | 'canvas';
export const VIEWS: { id: View; label: string; icon: string }[] = [
  { id: 'sessions', label: 'Sessions', icon: 'terminal' },
  { id: 'prs', label: 'PRs', icon: 'pr' },
  { id: 'todo', label: 'Todo', icon: 'list-check' },
  { id: 'schedules', label: 'Schedules', icon: 'calendar' },
  { id: 'kanban', label: 'Kanban', icon: 'layout-kanban' },
  { id: 'canvas', label: 'Canvas', icon: 'layout-grid' },
];
/** The views while boards are off (`[experimental] boards`): none of the Board view's. */
export const VIEWS_SANS_BOARDS = VIEWS.filter((v) => v.id !== 'kanban' && v.id !== 'canvas');
/** What each view says until the engine has read this Mac's board once. */
export const READING: Record<View, string> = {
  sessions: 'Reading sessions…',
  prs: 'Reading repos…',
  todo: 'Reading todos…',
  schedules: 'Reading schedules…',
  kanban: 'Reading todos…',
  canvas: 'Reading sessions…',
};
/** What the list's find says it finds in, in each view (ui/ListBar.tsx). */
export const FIND_IN: Record<View, string> = {
  sessions: 'Find in sessions',
  prs: 'Find in PRs',
  todo: 'Find in todos',
  schedules: 'Find in schedules',
  kanban: 'Find in todos and sessions',
  canvas: 'Find on the canvas',
};
/** How recently a worktree or session must have been active to show. */
export type Age = '1d' | '3d' | '7d' | 'all';
export const AGES: { id: Age; label: string; title: string; ms: number }[] = [
  { id: '1d', label: '24h', title: 'Active in the last 24 hours', ms: DAY },
  { id: '3d', label: '3d', title: 'Active in the last 3 days', ms: 3 * DAY },
  { id: '7d', label: '7d', title: 'Active in the last 7 days', ms: 7 * DAY },
  { id: 'all', label: 'All', title: 'Everything from the last 14 days', ms: 0 },
];
