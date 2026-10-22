// The Board view while the project has no boards: what a board is, and the way to make one. A
// project has only the boards made in it, none until one is (boards/model.ts).

import { ALL_PROJECTS } from '../quick';
import { Button, Icon } from './primitives';

interface BoardEmptyProps {
  /** The project the window shows. */
  project: string;
  /** Why its boards couldn't be read, when they couldn't: then that's all it says. */
  error: string | null;
  onNew: () => void;
}

export function BoardEmpty({ project, error, onNew }: BoardEmptyProps) {
  if (error) {
    return (
      <div className="ca-board-empty" role="alert">
        Boards can’t be read: {error}
      </div>
    );
  }
  const every = project === ALL_PROJECTS;
  return (
    <section className="ca-boardempty" aria-label="No boards">
      <Icon name="layout-kanban" size={28} className="ca-boardempty-icon" />
      <h2 className="ca-boardempty-title">{every ? 'No boards yet' : `No boards in ${project} yet`}</h2>
      <p className="ca-boardempty-text">
        A board is a set of work you pick out: todos, sessions, worktrees and PRs. It takes them in by a rule, such as a label, a repo or what waits on you, or as you add them, and shows them in Kanban columns or laid out on a canvas.
      </p>
      {every && <p className="ca-boardempty-text">A board made under All projects shows every project’s work.</p>}
      <Button variant="primary" icon="plus" onClick={onNew}>
        New board…
      </Button>
      <p className="ca-boardempty-hint">Or right-click a session, todo or PR and pick Add to board › New board with it…</p>
    </section>
  );
}
