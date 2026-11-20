// A PR's menus in the main window (App.tsx): its right-click menu as every window has it
// (menus.ts `prMenu`) with this window's own (approving it, its worktree shown, its pin, its
// place on a board), the same menu where a worktree or a session shows the PR on a chip, and
// Review with… dropped from a button. Review with… goes by where each account can start a
// session, which is read as the window opens and again each time a menu is built, so a menu goes
// by the last answer and never waits on one.

import { useCallback, useEffect, useRef } from 'react';
import { startOptions } from './api';
import type { RefKind } from './canvas/model';
import { below, showContextMenu, type MenuAction, type MenuSpec } from './contextMenu';
import * as menus from './menus';
import { clearNotice, reportNotice } from './notices';
import type { Pins } from './pins';
import { mayApprove } from './prStatus';
import { repoReviews } from './repoSettings';
import { errorText, type Report } from './report';
import { reviewItems as reviewItemsFor } from './review';
import { reviewSnoozeMenu } from './snooze';
import { START_OPTIONS_NOTICE, startOptionsFailed } from './start';
import type { Board, Lane, Pr, PrItem, StartOptions } from './types';
import type { PrRowItem } from './ui/PrsView';

/** A PR as Review with… needs it: one the board lists, or what a worktree knows of one it doesn't. */
type Reviewed = Pick<PrItem, 'number' | 'url' | 'repoSlug' | 'lanes'> & Partial<Pick<PrItem, 'key' | 'mine'>>;

/** What the window has for them. */
export interface PrMenusInput {
  board: Board | null;
  /** The worktrees the views list. */
  lanes: Lane[];
  report: Report;
  setToast: (message: string) => void;
  /** Shows that work is under way while it takes a moment. */
  track: <T>(label: string, work: Promise<T>) => Promise<T>;
  /** Approves a PR as you, in one click. */
  approve: (pr: PrItem) => void;
  view: string;
  /** Selects a PR's row alone, in PRs. */
  selectPr: (row: PrRowItem) => void;
  prPins: Pins;
  onDesktop: ReadonlySet<string>;
  /** Add to board › for one thing. */
  boardItems: (ref: RefKind, keys: string[]) => MenuSpec;
  /** A PR as the PRs view lists it. */
  prRowOf: (pr: PrItem) => PrRowItem;
  /** Shows a worktree where the view shows one, from `row` where a row's menu asked. */
  focusLane: (lane: Lane, row?: { key: string; select: () => void }) => void;
}

export function usePrMenus(input: PrMenusInput) {
  const { board, lanes, report, setToast, track, approve, view, selectPr, prPins, onDesktop, boardItems, prRowOf, focusLane } = input;
  // Where each account can start a session, for Review with: read at launch, and again each time
  // the menu is built, so a menu goes by the last answer and never waits on one.
  const startOpts = useRef<StartOptions | null>(null);
  const loadStartOpts = useCallback(() => {
    startOptions()
      .then((o) => {
        startOpts.current = o;
        clearNotice(START_OPTIONS_NOTICE);
      })
      // Keeps the last answer, and says so until a read works.
      .catch((e: unknown) => reportNotice(startOptionsFailed(e)));
  }, []);
  useEffect(loadStartOpts, [loadStartOpts]);
  /** Review with…'s items for a PR: a round of Codealong's own reviews on an account, or its
   *  review page in the Codex app (review.ts). */
  const reviewItems = (pr: Reviewed): MenuAction[] => {
    loadStartOpts();
    return reviewItemsFor(pr, lanes, startOpts.current?.accounts ?? [], board?.usage ?? [], { error: setToast, info: setToast, busy: track }, undefined, Date.now(), repoReviews(board, pr.repoSlug), startOpts.current?.groups ?? []);
  };
  /** A worktree's PR as Review with… needs it: the board's, or what the worktree knows of one
   *  the board doesn't list (on another Mac, say). */
  const reviewedPr = (lane: Lane, p: Pr) => board?.prs.find((x) => x.url === p.url) ?? { ...p, repoSlug: lane.repoSlugNow ?? lane.repoSlug ?? '', lanes: [lane.id] };
  /** Drops Review with… from a button, as the PR panels have it. */
  const reviewMenuFrom = (pr: Reviewed, el: Element) => {
    const items = reviewItems(pr);
    const spec = items.length > 0 ? items : [{ label: 'No account can start a review here', run: () => {}, enabled: false }];
    void showContextMenu(spec, below(el)).catch((e: unknown) => setToast(String(e)));
  };

  /** Snooze review, for a PR that asks for your review: the times it can wait, and Wake now. */
  const snoozeReview = (p: PrItem) => reviewSnoozeMenu(p, Date.now(), (act) => void act.catch((e: unknown) => setToast(errorText(e))));

  /** A PR's right-click menu, as every window has it (menus.ts), with this window's own. */
  const prMenuSpec = (r: PrRowItem): MenuSpec => {
    const p = r.pr;
    const lane = r.lane;
    return menus.prMenu(p, {
      report,
      review: reviewItems(p),
      approve: mayApprove(p) ? () => approve(p) : undefined,
      snooze: snoozeReview(p),
      showLane: lane ? { lane, run: () => focusLane(lane, view === 'prs' ? { key: r.key, select: () => selectPr(r) } : undefined) } : undefined,
      pin: { label: prPins.set.has(p.key) ? 'Unpin PR' : 'Pin PR', run: () => prPins.toggle(p.key), accelerator: menus.KEYS.pin },
      desktop: onDesktop,
      boards: boardItems('pr', [p.key]),
      lanes,
    });
  };
  const prMenu = (r: PrRowItem, at?: { x: number; y: number }) => void showContextMenu(prMenuSpec(r), at).catch((e: unknown) => setToast(String(e)));
  /** The right-click menu of a worktree's PR chip: the PR's own. */
  const lanePrMenu = (lane: Lane) => {
    if (lane.pr) prChipMenu(lane, lane.pr);
  };
  /** A PR's menu where a worktree or one of its sessions shows it. A PR the board doesn't list,
   *  such as one on another Mac in a repo this one doesn't have, gets what its link allows. */
  const prChipMenu = (lane: Lane, p: Pr) => {
    const item = board?.prs.find((x) => x.url === p.url);
    const spec: MenuSpec = item ? prMenuSpec(prRowOf(item)) : menus.prMenu(p, { report, review: reviewItems(reviewedPr(lane, p)) });
    void showContextMenu(spec).catch((e: unknown) => setToast(String(e)));
  };
  return { reviewedPr, reviewMenuFrom, prMenuSpec, prMenu, lanePrMenu, prChipMenu };
}
