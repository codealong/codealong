// The card on a session's helpers chip (LaneRow.tsx `HelpersChip`), up while the pointer rests on
// the chip or on it (HoverCard.tsx): every helper the session's agent started, and the automatic
// reviews of a PR it opened, the most urgent first, each with where it stands since when, what
// it's doing, asking or last said, and who runs it, on which model, where. A click on one shows it in
// Sessions, under the session, with its conversation in the panel; its arrow opens it where it
// runs; a right-click has its menu. Its button lists them all under the session, or only the ones
// at work, as the chip's double-click does.

import type { HelperCount } from '../boardLists';
import { absolute } from '../format';
import { helperName, helperStands } from '../lanes';
import { HoverCard, type HoverCardState } from './HoverCard';
import { Button, Icon, Rich, SessionGlyph } from './primitives';

/** What the card does with a helper, by its session's key. */
export interface HelperActions {
  /** Shows it in Sessions, listed under its session, with its conversation in the panel. */
  show: (key: string) => void;
  /** Opens it where it runs. */
  open: (key: string) => void;
  menu: (key: string) => void;
}

/** Words that start a line, as a sentence does. */
const sentence = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);

interface HelpersCardProps {
  card: HoverCardState;
  helpers: HelperCount;
  now: number;
  /** Whether every one is listed under the session, finished ones too (`HelpersChip`'s `shown`). */
  shown: boolean;
  onToggle?: () => void;
  actions: HelperActions;
}

export function HelpersCard({ card, helpers: h, now, shown, onToggle, actions }: HelpersCardProps) {
  // Doing something from the card closes it first.
  const act = (f: () => void) => {
    card.close();
    f();
  };
  return (
    <HoverCard card={card} label="Its helpers" className="ca-helperscard">
      <div className="ca-hovercard-head">
        <Icon name="subtask" size={14} />
        <span className="ca-helperscard-count">{h.count === 1 ? 'Its helper' : `Its ${h.count} helpers`}</span>
      </div>
      <p className="ca-hovercard-meta">Working for this session: they're listed under it while they work or wait on you.</p>
      <div className="ca-helperscard-list" role="list">
        {h.each.map((g) => {
          const name = helperName(g);
          const runs = [g.account, g.model, g.worktree && `in ${g.worktree}`, g.reports && 'tells it when done'].filter(Boolean).join(' · ');
          return (
            <div
              key={g.key}
              role="listitem"
              className="ca-helperscard-row"
              onContextMenu={(e) => {
                e.preventDefault();
                act(() => actions.menu(g.key));
              }}
            >
              <button type="button" className="ca-helperscard-main" title={`${g.title}\nClick to show it here`} onClick={() => act(() => actions.show(g.key))}>
                <SessionGlyph s={g} size={10} />
                <span className="ca-helperscard-text">
                  <span className="ca-helperscard-name">{name}</span>
                  <span className="ca-helperscard-stands" title={absolute(g.since)}>
                    {sentence(helperStands(g, now))}
                  </span>
                  {g.excerpt && (
                    <span className="ca-helperscard-says">
                      <Rich text={g.excerpt} now={now} />
                    </span>
                  )}
                  {runs && <span className="ca-helperscard-runs">{runs}</span>}
                </span>
              </button>
              <button type="button" className="ca-helperscard-open" aria-label="Open where it runs" title="Open it where it runs" onClick={() => act(() => actions.open(g.key))}>
                <Icon name="arrow-up-right" size={12} />
              </button>
            </div>
          );
        })}
      </div>
      {onToggle && (
        <div className="ca-hovercard-foot">
          <Button size="sm" icon="layout-rows" onClick={() => act(onToggle)}>
            {shown ? 'List only the ones at work' : 'List them all under it'}
          </Button>
        </div>
      )}
    </HoverCard>
  );
}
