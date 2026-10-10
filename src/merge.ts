// A PR's merge box (ui/MergeBox.tsx), as GitHub's has it: what it says of where the PR stands,
// what it offers you, the words for each way of merging, and what each act asks of the engine
// (prmerge.rs reads the box and does the acts).
//
// It merges now when GitHub would: the PR is open and out of draft, you can write to the repo, and
// GitHub says it's clean, or unstable (checks failed or still running that the repo doesn't
// require). Otherwise it offers what GitHub offers then: auto-merge, which merges it once its
// requirements are met; the merge queue, where the base branch merges through one; merging past
// the rules, for one who may. A branch behind its base can be brought up to date, a draft made
// ready, and a merged or closed PR's branch deleted. The method is picked per repo and kept, GitHub's
// own pick for you until then.

import type { Codec } from './stored';
import type { Checks, MergeBox, MergeMethod, MergeOp, PrItem } from './types';

/** The words for each way of merging, as GitHub's box says them. */
export const METHOD: Record<MergeMethod, { label: string; button: string; confirm: string; verb: string; done: string; what: string }> = {
  merge: {
    label: 'Create a merge commit',
    button: 'Merge',
    confirm: 'Confirm merge',
    verb: 'merge',
    done: 'Merged',
    what: 'Every commit on the branch goes onto the base, joined by a merge commit.',
  },
  squash: {
    label: 'Squash and merge',
    button: 'Squash and merge',
    confirm: 'Confirm squash and merge',
    verb: 'squash and merge',
    done: 'Squashed and merged',
    what: 'The branch’s commits go onto the base as one commit.',
  },
  rebase: {
    label: 'Rebase and merge',
    button: 'Rebase and merge',
    confirm: 'Confirm rebase and merge',
    verb: 'rebase and merge',
    done: 'Rebased and merged',
    what: 'The branch’s commits go onto the base one by one, with no merge commit.',
  },
};

/** How the box reads at a glance: ready, waiting on something, held back by something to fix,
 *  or over (merged or closed). */
export type Tone = 'ok' | 'wait' | 'bad' | 'done';

export interface Standing {
  tone: Tone;
  says: string;
  /** What's behind it, on a line of its own. */
  more?: string;
}

const QUEUE_WORDS: Record<string, string> = {
  QUEUED: 'Waiting its turn.',
  AWAITING_CHECKS: 'Waiting for the queue’s checks.',
  MERGEABLE: 'The queue’s checks passed: it merges next.',
  UNMERGEABLE: 'It can’t merge, and will leave the queue.',
  LOCKED: 'Merging now.',
};

/** What the checks say, where the box says it. */
function checksWords(c: Checks): string[] {
  return [c.failed > 0 && `${c.failed} ${c.failed === 1 ? 'check' : 'checks'} failed`, c.pending > 0 && `${c.pending} running`].filter((x): x is string => !!x);
}

/** What was said, as one clause with its first letter a capital. */
function clause(parts: string[]): string {
  const s = parts.join(', ');
  return s && `${s.charAt(0).toUpperCase()}${s.slice(1)}`;
}

/** Where the PR stands for merging, as the box says it above what it offers. */
export function standing(box: MergeBox, pr: Pick<PrItem, 'base' | 'checks' | 'repoSlug'>): Standing {
  const base = pr.base ?? 'its base';
  if (box.state === 'MERGED') return { tone: 'done', says: box.mergedBy ? `Merged by ${box.mergedBy}` : 'Merged' };
  if (box.state === 'CLOSED') return { tone: 'done', says: 'Closed without merging' };
  const cant = box.canMerge ? undefined : `Only people who can write to ${pr.repoSlug} can merge it.`;
  if (box.draft) return { tone: 'wait', says: 'Still a draft', more: 'GitHub doesn’t merge a draft: mark it ready for review first.' };
  if (box.queued) return { tone: 'wait', says: 'In the merge queue', more: QUEUE_WORDS[box.queued.state ?? ''] };
  if (box.auto) {
    const how = box.auto.method ? METHOD[box.auto.method].verb : 'merge';
    return { tone: 'wait', says: 'Auto-merge is on', more: `GitHub will ${how} it once its requirements are met${box.auto.by ? `. Turned on by ${box.auto.by}` : ''}.` };
  }
  if (box.mergeable === 'CONFLICTING' || box.mergeState === 'DIRTY') return { tone: 'bad', says: `Conflicts with ${base}`, more: 'They need resolving before it can merge.' };
  switch (box.mergeState) {
    case 'BEHIND':
      return { tone: 'wait', says: `Behind ${base}`, more: cant ?? 'The repo wants the branch brought up to date before it merges.' };
    case 'BLOCKED': {
      const why = [
        box.reviewDecision === 'CHANGES_REQUESTED' && 'changes were asked for',
        box.reviewDecision === 'REVIEW_REQUIRED' && 'it needs an approving review',
        pr.checks.failed > 0 && 'checks failed',
        pr.checks.failed === 0 && pr.checks.pending > 0 && 'checks are still running',
      ].filter((x): x is string => !!x);
      return { tone: pr.checks.failed > 0 || box.reviewDecision === 'CHANGES_REQUESTED' ? 'bad' : 'wait', says: 'Merging is blocked', more: cant ?? (why.length > 0 ? `${clause(why)}.` : 'The repo’s rules hold it back.') };
    }
    case 'UNSTABLE':
      return { tone: 'ok', says: 'Ready to merge', more: cant ?? `${clause(checksWords(pr.checks)) || 'Some checks haven’t passed'}, which the repo doesn’t require.` };
    case 'CLEAN':
    case 'HAS_HOOKS': {
      const good = [box.reviewDecision === 'APPROVED' && 'approved', pr.checks.passed > 0 && `${pr.checks.passed} ${pr.checks.passed === 1 ? 'check' : 'checks'} passed`].filter((x): x is string => !!x);
      return { tone: 'ok', says: 'Ready to merge', more: cant ?? (good.length > 0 ? `${clause(good)}.` : undefined) };
    }
    default:
      return { tone: 'wait', says: 'Checking whether it can merge…', more: cant };
  }
}

/** What the box offers you now. */
export interface Offers {
  /** Merge it now. */
  merge: boolean;
  /** Merge it now, past the rules: the box's bypass checkbox. */
  bypass: boolean;
  auto: boolean;
  stopAuto: boolean;
  queue: boolean;
  dequeue: boolean;
  /** Bring its branch up to date with its base. */
  update: boolean;
  ready: boolean;
  /** Its branch, once it's merged or closed. */
  deleteBranch: boolean;
  /** Its conflicts, resolved on GitHub. */
  conflicts: boolean;
}

export function offers(box: MergeBox): Offers {
  const open = box.state === 'OPEN';
  const conflicts = open && (box.mergeable === 'CONFLICTING' || box.mergeState === 'DIRTY');
  const settled = open && !box.draft && !box.queued && !box.auto;
  const merge = settled && box.canMerge && !box.queue && box.methods.length > 0 && ['CLEAN', 'HAS_HOOKS', 'UNSTABLE'].includes(box.mergeState);
  return {
    merge,
    bypass: settled && box.canBypass && !merge && !conflicts && box.methods.length > 0,
    auto: settled && box.canAuto && !box.queue && box.methods.length > 0,
    stopAuto: open && !!box.auto && box.canStopAuto,
    queue: settled && box.canMerge && box.queue && !conflicts,
    dequeue: open && !!box.queued && box.canMerge,
    update: open && box.canUpdateBranch,
    ready: open && box.draft && box.canReady,
    deleteBranch: !open && !!box.branchRef,
    conflicts,
  };
}

/** What the box's main button does: merge now (past the rules, with the box ticked), add it to
 *  the queue, or turn on auto-merge; null when it offers none of them. */
export type MergeMode = 'merge' | 'bypass' | 'queue' | 'auto';

export function mainMode(o: Offers, bypassing: boolean): MergeMode | null {
  if (o.merge) return 'merge';
  if (o.bypass && bypassing) return 'bypass';
  if (o.queue) return 'queue';
  if (o.auto) return 'auto';
  return null;
}

/** What the main button says. */
export function modeLabel(mode: MergeMode, method: MergeMethod): string {
  if (mode === 'queue') return 'Add to merge queue';
  if (mode === 'auto') return 'Enable auto-merge';
  if (mode === 'bypass') return 'Bypass rules and merge';
  return METHOD[method].button;
}

/** What the confirm button says, once the commit is written. */
export function confirmLabel(mode: MergeMode, method: MergeMethod): string {
  return mode === 'auto' ? 'Confirm auto-merge' : METHOD[method].confirm;
}

/** What a PR's box is read for: the PR as last updated, and what the board says of where it stands
 *  for merging, its state (its review in it), GitHub's merge status and its checks, which change
 *  without the PR being updated, as when its checks finish. A box read for another is read again. */
export function boxKey(pr: Pick<PrItem, 'key' | 'updatedAt' | 'state' | 'merge' | 'checks'>): string {
  const c = pr.checks;
  return `${pr.key}@${pr.updatedAt}:${pr.state}:${pr.merge}:${c.passed}/${c.failed}/${c.pending}`;
}

/** Whether a box just read is worth reading again in a moment: GitHub hasn't worked out yet whether
 *  the PR can merge, or doesn't yet say what the board read, that it's ready. */
export function rereads(box: Pick<MergeBox, 'state' | 'mergeState'>, pr: Pick<PrItem, 'merge'>): boolean {
  if (box.state !== 'OPEN') return false;
  return box.mergeState === 'UNKNOWN' || (pr.merge === 'clean' && box.mergeState !== 'CLEAN' && box.mergeState !== 'HAS_HOOKS');
}

/** The method the box starts on: the one you picked in this repo before, else GitHub's pick for
 *  you, either only while the repo still allows it. */
export function pickMethod(box: Pick<MergeBox, 'methods' | 'defaultMethod'>, picked?: MergeMethod | null): MergeMethod | null {
  if (picked && box.methods.includes(picked)) return picked;
  return box.defaultMethod ?? box.methods[0] ?? null;
}

/** The commit's title and message as GitHub would write them for a method; a rebase writes none. */
export function commitText(box: Pick<MergeBox, 'mergeText' | 'squashText'>, method: MergeMethod) {
  return method === 'merge' ? box.mergeText : method === 'squash' ? box.squashText : null;
}

/** What's filled in before the confirm button. */
export interface MergeForm {
  method: MergeMethod;
  title: string;
  body: string;
  email: string | null;
  deleteBranch: boolean;
}

/** The branch can go with the merge: GitHub doesn't delete it itself, and you can push to it. */
export function deletesWith(box: Pick<MergeBox, 'deletesBranch' | 'branchRef'>, mode: MergeMode): boolean {
  return (mode === 'merge' || mode === 'bypass') && !box.deletesBranch && !!box.branchRef;
}

/** What the main button, confirmed, asks of the engine. */
export function opFor(box: MergeBox, mode: MergeMode, form: MergeForm): MergeOp {
  if (mode === 'queue') return { op: 'enqueue', head: box.head };
  const writes = form.method !== 'rebase';
  const commit = { method: form.method, title: writes ? form.title : null, body: writes ? form.body : null, email: writes && box.emails.length > 0 ? form.email : null };
  if (mode === 'auto') return { op: 'auto', head: box.head, ...commit };
  return { op: 'merge', head: box.head, deleteBranch: form.deleteBranch && deletesWith(box, mode) ? box.branchRef : null, ...commit };
}

/** What the box says once GitHub has done it. */
export function doneWords(op: MergeOp, number: number, branch: string): string {
  switch (op.op) {
    case 'merge':
      return `${METHOD[op.method].done} #${number}${op.deleteBranch ? ` and deleted ${branch}` : ''}.`;
    case 'auto':
      return `Auto-merge is on for #${number}.`;
    case 'stop-auto':
      return `Auto-merge is off for #${number}.`;
    case 'enqueue':
      return `#${number} is in the merge queue.`;
    case 'dequeue':
      return `#${number} is out of the merge queue.`;
    case 'update-branch':
      return `Brought ${branch} up to date${op.rebase ? ' by a rebase' : ''}.`;
    case 'ready':
      return `#${number} is ready for review.`;
    case 'delete-branch':
      return `Deleted ${branch}.`;
  }
}

/** The method picked in each repo, by its slug: what a merge there starts on next time. */
export const METHODS_KEY = 'codealong.merge.methods';
export const methodsByRepo: Codec<Record<string, MergeMethod>> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object' || Array.isArray(v)) return undefined;
    return Object.fromEntries(Object.entries(v).filter((e): e is [string, MergeMethod] => e[1] === 'merge' || e[1] === 'squash' || e[1] === 'rebase'));
  },
  format: (value) => JSON.stringify(value),
};

/** Whether a merge deletes the branch with it, the last way you left the box. */
export const DELETE_KEY = 'codealong.merge.deleteBranch';
