// The card on a session's helpers chip (LaneRow.tsx `HelpersChip`), up while the pointer rests on
// the chip or on it (HoverCard.tsx): every helper the session's agent started, and the automatic
// reviews of a PR it opened, the most urgent first, each with where it stands since when, what
// it's doing, asking or last said, and who runs it, on which model, where. A click on one shows it in
// Sessions, under the session, with its conversation in the panel; its arrow opens it where it
// runs; a right-click has its menu. Its button lists them all under the session, or only the ones
// at work, as the chip's double-click does. The headless runs chip (`RunsChip`) has the same card
// for the runs its commands started (`kind`), which aren't listed while at work; an eval leaves
// hundreds, so the card shows the first `MAX_ROWS`. The side conversations chip (`SidesChip`)
// has it for the questions asked about the session on the side: a click on one shows it above
// the session's box, and they run nowhere else, so have no arrow and no menu.

import type { HelperCount } from '../boardLists';
import { absolute, agoPhrase, duration } from '../format';
import { helperName, helperStands, type HelperGlance } from '../lanes';
import { HoverCard, type HoverCardState } from './HoverCard';
import { Button, Icon, Rich, SessionGlyph } from './primitives';

/** What the card does with a helper, by its session's key. */
export interface HelperActions {
  /** Shows it in Sessions, listed under its session, with its conversation in the panel. */
  show: (key: string) => void;
  /** Opens it where it runs: none for what runs nowhere but here (a side conversation). */
  open?: (key: string) => void;
  menu?: (key: string) => void;
}

/** How many rows the card shows: the rest are listed under the session from its button. */
const MAX_ROWS = 20;

/** What the card says of each kind: its heading, what they are, and its button. */
const KINDS = {
  helpers: {
    icon: 'subtask',
    label: 'Its helpers',
    head: (n: number) => (n === 1 ? 'Its helper' : `Its ${n} helpers`),
    meta: 'Working for this session: they’re listed under it while they work or wait on you.',
    list: (shown: boolean) => (shown ? 'List only the ones at work' : 'List them all under it'),
  },
  runs: {
    icon: 'robot',
    label: 'Its headless runs',
    head: (n: number) => (n === 1 ? 'Its headless run' : `Its ${n} headless runs`),
    meta: 'Run by its commands with nobody at them (claude -p, codex exec), which read what they say: counted on its row, not listed, unless one waits on you.',
    list: (shown: boolean) => (shown ? 'List none of them under it' : 'List them all under it'),
  },
  sides: {
    icon: 'help',
    label: 'Its side conversations',
    head: (n: number) => (n === 1 ? 'Its side conversation' : `Its ${n} side conversations`),
    meta: 'Questions about it that a throwaway copy of its conversation answered: the session never saw them. The one answering is listed under it; a click shows one above its box.',
    list: (shown: boolean) => (shown ? 'List only the one answering' : 'List them all under it'),
  },
} as const;

/** Where one stands, since when, as the card says it of each kind. */
const stands = (kind: keyof typeof KINDS, g: HelperGlance, now: number): string =>
  kind !== 'sides' ? helperStands(g, now) : g.status === 'working' ? `answering for ${duration(g.since, now)}` : `answered ${agoPhrase(g.since, now)}`;

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
  /** Its session's helpers (the default), or the headless runs its commands started. */
  kind?: keyof typeof KINDS;
}

export function HelpersCard({ card, helpers: h, now, shown, onToggle, actions, kind = 'helpers' }: HelpersCardProps) {
  const k = KINDS[kind];
  const more = h.each.length - MAX_ROWS;
  // Doing something from the card closes it first.
  const act = (f: () => void) => {
    card.close();
    f();
  };
  return (
    <HoverCard card={card} label={k.label} className="ca-helperscard">
      <div className="ca-hovercard-head">
        <Icon name={k.icon} size={14} />
        <span className="ca-helperscard-count">{k.head(h.count)}</span>
      </div>
      <p className="ca-hovercard-meta">{k.meta}</p>
      <div className="ca-helperscard-list" role="list">
        {h.each.slice(0, MAX_ROWS).map((g) => {
          const name = helperName(g);
          const runs = [g.account, g.model, g.worktree && `in ${g.worktree}`, g.reports && 'tells it when done'].filter(Boolean).join(' · ');
          return (
            <div
              key={g.key}
              role="listitem"
              className="ca-helperscard-row"
              onContextMenu={
                actions.menu &&
                ((e) => {
                  e.preventDefault();
                  act(() => actions.menu?.(g.key));
                })
              }
            >
              <button type="button" className="ca-helperscard-main" title={`${g.title}\nClick to show it here`} onClick={() => act(() => actions.show(g.key))}>
                <SessionGlyph s={g} size={10} />
                <span className="ca-helperscard-text">
                  <span className="ca-helperscard-name">{name}</span>
                  <span className="ca-helperscard-stands" title={absolute(g.since)}>
                    {sentence(stands(kind, g, now))}
                  </span>
                  {g.excerpt && (
                    <span className="ca-helperscard-says">
                      <Rich text={g.excerpt} now={now} />
                    </span>
                  )}
                  {runs && <span className="ca-helperscard-runs">{runs}</span>}
                </span>
              </button>
              {actions.open && (
                <button type="button" className="ca-helperscard-open" aria-label="Open where it runs" title="Open it where it runs" onClick={() => act(() => actions.open?.(g.key))}>
                  <Icon name="arrow-up-right" size={12} />
                </button>
              )}
            </div>
          );
        })}
      </div>
      {more > 0 && <p className="ca-hovercard-meta">And {more} more, which its button lists.</p>}
      {onToggle && (
        <div className="ca-hovercard-foot">
          <Button size="sm" icon="layout-rows" onClick={() => act(onToggle)}>
            {k.list(shown)}
          </Button>
        </div>
      )}
    </HoverCard>
  );
}
