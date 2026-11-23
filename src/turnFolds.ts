// Pure rules for summarizing tool runs and folding completed turns in a session's conversation
// (SessionThread.tsx, toolDisplay.ts). Kept separate so SessionThread stays focused on rendering
// and scroll management, and every folding rule is pinned by unit tests (turnFolds.test.ts).

import { shownText } from './threadFind';
import type { Question, ToolDisplay, TranscriptItem } from './types';
import { says } from './windowFind';

export interface ToolCall {
  item: TranscriptItem;
  result?: TranscriptItem;
}

export type BaseRow =
  | { kind: 'item'; item: TranscriptItem; result?: TranscriptItem }
  | { kind: 'tools'; calls: ToolCall[] }
  /** Your answers to the questions a call asked you, on its result. */
  | { kind: 'answers'; result: TranscriptItem; questions: Question[] };

export type ThreadRow =
  | BaseRow
  /** A completed turn's intermediate work (tool calls and intermediate agent updates) folded into
   *  one quiet expandable row above the turn's final reply. When `hidden` is true (zen mode), it
   *  stays out of sight unless ⌘F matches something inside it. */
  | { kind: 'turn'; id: string; rows: ThreadRow[]; calls: number; summary: string; took: string | null; hidden: boolean };

/** Tool calls in a row before they fold in detailed mode. */
export const FOLD_AT_DETAILED = 3;
/** Tool calls in a row before they fold into one summary line in compact / folded modes. */
export const FOLD_AT_COMPACT = 2;

export const itemKey = (it: TranscriptItem) => `${it.cursor}:${it.n}`;

/** A call's result that has your answers to the questions it asked. */
export const hasAnswers = (it: TranscriptItem) => it.kind === 'result' && Object.keys(it.answers ?? {}).length > 0;

/** Whether a tool call failed, on itself or on its result. */
export const callFailed = (c: ToolCall) => c.item.failed || !!c.result?.failed;

/** A tool call that always stays out of tool runs and turn folds: published artifacts, files sent
 *  to you, and questions asked of you. */
export const isPinnedCall = (c: ToolCall) => !!(c.item.link || c.result?.link || c.item.sent || c.item.questions?.length);

type ActionKind = 'edit' | 'command' | 'agent' | 'skill' | 'search' | 'read' | 'other';

const ACTION_PRIORITY: Record<ActionKind, number> = {
  edit: 0,
  command: 1,
  agent: 2,
  skill: 3,
  search: 4,
  read: 5,
  other: 6,
};

/** Classifies a tool call by the tool name at the start of its label (`Bash · ...`). */
export function classifyTool(text: string): ActionKind {
  const name = text.split(' · ')[0].trim().toLowerCase();
  if (/edit|write|patch|notebook|replace|create_file/.test(name)) return 'edit';
  if (/bash|shell|exec|command|terminal|^run\b/.test(name)) return 'command';
  if (/agent|task|subagent/.test(name)) return 'agent';
  if (/skill/.test(name)) return 'skill';
  if (/grep|glob|search|find/.test(name)) return 'search';
  if (/read|view/.test(name)) return 'read';
  return 'other';
}

/** The icon for a summarized group of tool calls, by its highest-priority action kind. */
export function summaryIcon(calls: ToolCall[]): string {
  let best: ActionKind = 'other';
  for (const c of calls) {
    const k = classifyTool(c.item.text);
    if (ACTION_PRIORITY[k] < ACTION_PRIORITY[best]) best = k;
  }
  switch (best) {
    case 'edit':
      return 'code';
    case 'command':
      return 'terminal';
    case 'agent':
      return 'sparkles';
    case 'search':
      return 'search';
    case 'read':
      return 'file';
    default:
      return 'bolt';
  }
}

function actionCount(kind: ActionKind, calls: ToolCall[]): number {
  if (kind !== 'edit' && kind !== 'read') return calls.length;
  const files = new Set<string>();
  let unnamed = 0;
  for (const c of calls) {
    if (c.item.file) files.add(c.item.file);
    else unnamed += 1;
  }
  return files.size + unnamed;
}

function actionPhrase(kind: ActionKind, count: number): string {
  switch (kind) {
    case 'edit':
      return count === 1 ? 'Edited 1 file' : `Edited ${count} files`;
    case 'command':
      return count === 1 ? 'Ran 1 command' : `Ran ${count} commands`;
    case 'agent':
      return count === 1 ? 'Ran a subagent' : `Ran ${count} subagents`;
    case 'skill':
      return count === 1 ? 'Used a skill' : `Used ${count} skills`;
    case 'search':
      return count === 1 ? 'Searched once' : `Searched ${count} times`;
    case 'read':
      return count === 1 ? 'Read 1 file' : `Read ${count} files`;
    case 'other':
      return count === 1 ? '1 tool call' : `${count} tool calls`;
  }
}

function joinClauses(clauses: string[]): string {
  const lowered = clauses.map((c, i) => (i === 0 ? c : c[0].toLowerCase() + c.slice(1)));
  if (lowered.length <= 1) return lowered[0] ?? '';
  if (lowered.length === 2) return `${lowered[0]} and ${lowered[1]}`;
  return `${lowered.slice(0, -1).join(', ')} and ${lowered[lowered.length - 1]}`;
}

/** A concise plain-English summary of a group of tool calls: up to two prioritized action
 *  categories plus a count of remaining actions, and a failed count when any failed. */
export function summarizeTools(calls: ToolCall[]): string {
  if (calls.length === 0) return '';
  const groups = new Map<ActionKind, { index: number; calls: ToolCall[] }>();
  let order = 0;
  for (const c of calls) {
    const kind = classifyTool(c.item.text);
    const existing = groups.get(kind);
    if (existing) existing.calls.push(c);
    else groups.set(kind, { index: order++, calls: [c] });
  }
  const entries = [...groups.entries()].map(([kind, g]) => ({
    kind,
    index: g.index,
    calls: g.calls,
    priority: ACTION_PRIORITY[kind],
    label: actionPhrase(kind, actionCount(kind, g.calls)),
  }));
  const picked = [...entries]
    .sort((a, b) => a.priority - b.priority || a.index - b.index)
    .slice(0, 2)
    .sort((a, b) => a.index - b.index);
  const labels = picked.map((e) => e.label);
  const covered = picked.reduce((n, e) => n + e.calls.length, 0);
  const rest = calls.length - covered;
  if (rest > 0) labels.push(rest === 1 ? '1 other action' : `${rest} other actions`);
  const failed = calls.filter(callFailed).length;
  const base = joinClauses(labels);
  return failed > 0 ? `${base} · ${failed} failed` : base;
}

/** Concise elapsed time for a completed turn fold: `12s`, `2m`, `1h 5m`, or null when too short or
 *  missing timestamps. */
export function formatWorked(ms: number): string | null {
  if (!Number.isFinite(ms) || ms < 1000) return null;
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem > 0 ? `${hours}h ${rem}m` : `${hours}h`;
}

/** Joins tool results to their calls, folds consecutive tool runs according to `mode`, and places
 *  your answers to questions where you gave them. */
export function rowsOf(items: TranscriptItem[], mode: ToolDisplay = 'detailed'): BaseRow[] {
  const results = new Map<string, TranscriptItem>();
  for (const it of items) if (it.kind === 'result' && it.id) results.set(it.id, it);
  const asked = new Map<string, Question[]>();
  const rows: BaseRow[] = [];
  const foldAt = mode === 'detailed' ? FOLD_AT_DETAILED : FOLD_AT_COMPACT;
  const separateFailed = mode !== 'detailed';
  let run: ToolCall[] = [];
  const flush = () => {
    if (run.length >= foldAt) rows.push({ kind: 'tools', calls: run });
    else rows.push(...run.map((c) => ({ kind: 'item' as const, ...c })));
    run = [];
  };
  for (const it of items) {
    if (it.kind === 'result') {
      if (hasAnswers(it)) {
        flush();
        rows.push({ kind: 'answers', result: it, questions: (it.id && asked.get(it.id)) || [] });
      }
      continue;
    }
    if (it.kind === 'tool') {
      const call: ToolCall = { item: it, result: it.id ? results.get(it.id) : undefined };
      if (it.id && it.questions?.length) asked.set(it.id, it.questions);
      // A call that made a page, sent you files, or asked you questions stays in view; in compact,
      // folded and zen modes, failed calls stay out of a run fold too so errors are never hidden.
      if (isPinnedCall(call) || (separateFailed && callFailed(call))) {
        flush();
        rows.push({ kind: 'item', ...call });
      } else run.push(call);
      continue;
    }
    flush();
    rows.push({ kind: 'item', item: it, result: it.kind === 'you' && it.id ? results.get(it.id) : undefined });
  }
  flush();
  return rows;
}

const isBoundary = (r: ThreadRow): boolean =>
  r.kind === 'answers' || (r.kind === 'item' && (r.item.kind === 'you' || r.item.kind === 'codealong' || r.item.kind === 'peer' || r.item.kind === 'note'));

const boundaryAt = (r: ThreadRow): number => (r.kind === 'answers' ? r.result.at : r.kind === 'item' ? r.item.at : 0);

const rowHasFailure = (r: ThreadRow): boolean =>
  r.kind === 'tools' ? r.calls.some(callFailed) : r.kind === 'item' && r.item.kind === 'tool' && callFailed({ item: r.item, result: r.result });

const isPinnedInTurn = (r: ThreadRow): boolean =>
  r.kind === 'item' && r.item.kind === 'tool' && (isPinnedCall({ item: r.item, result: r.result }) || callFailed({ item: r.item, result: r.result }));

function callsIn(rows: ThreadRow[]): ToolCall[] {
  const out: ToolCall[] = [];
  for (const r of rows) {
    if (r.kind === 'tools') out.push(...r.calls);
    else if (r.kind === 'item' && r.item.kind === 'tool') out.push({ item: r.item, result: r.result });
  }
  return out;
}

function firstKey(rows: ThreadRow[]): string {
  const r = rows[0];
  if (!r) return 'empty';
  if (r.kind === 'tools') return itemKey(r.calls[0].item);
  if (r.kind === 'answers') return itemKey(r.result);
  if (r.kind === 'item') return itemKey(r.item);
  return r.id;
}

function timeSpan(rows: ThreadRow[]): { first: number; last: number } {
  let first = 0;
  let last = 0;
  const take = (at: number) => {
    if (at <= 0) return;
    if (first === 0) first = at;
    last = at;
  };
  for (const r of rows) {
    if (r.kind === 'tools') {
      for (const c of r.calls) {
        take(c.item.at);
        if (c.result) take(c.result.at);
      }
    } else if (r.kind === 'item') {
      take(r.item.at);
      if (r.result) take(r.result.at);
    }
  }
  return { first, last };
}

function makeTurnFold(rows: ThreadRow[], startAt: number, endAt: number, hidden: boolean): ThreadRow {
  const calls = callsIn(rows);
  const span = timeSpan(rows);
  const from = startAt > 0 ? startAt : span.first;
  const to = endAt > 0 ? endAt : span.last;
  const took = from > 0 && to > from ? formatWorked(to - from) : null;
  let summary: string;
  if (calls.length > 0) {
    summary = summarizeTools(calls);
  } else {
    const updates = rows.filter((r) => r.kind === 'item' && r.item.kind === 'agent').length;
    summary = updates === 1 ? '1 earlier update' : `${updates} earlier updates`;
  }
  return { kind: 'turn', id: `turn:${firstKey(rows)}`, rows, calls: calls.length, summary, took, hidden };
}

/** Folds completed turns' intermediate tool calls and narration into a single expandable row above
 *  the turn's final reply (`folded` mode) or hides them until ⌘F matches (`zen` mode). Active
 *  turns, failed turns (in `folded` mode), questions, artifacts, and sent files stay unfolded. */
export function foldTurns(
  rows: ThreadRow[],
  opts: { mode: ToolDisplay; active: boolean; undrawn?: (it: TranscriptItem) => boolean },
): ThreadRow[] {
  if (opts.mode !== 'folded' && opts.mode !== 'zen') return rows;
  const undrawn = opts.undrawn ?? (() => false);
  const hidden = opts.mode === 'zen';

  // Split into segments separated by turn boundaries (prompts, answers, notes, cards).
  const out: ThreadRow[] = [];
  let segStart = 0;
  let startAt = 0;

  const emitSegment = (seg: ThreadRow[], isLast: boolean) => {
    if (seg.length === 0) return;
    // Never fold an active turn at the end of the conversation.
    if (isLast && opts.active) {
      out.push(...seg);
      return;
    }
    // In folded mode, a turn with any failed tool call stays expanded so errors are immediately clear.
    if (opts.mode === 'folded' && seg.some(rowHasFailure)) {
      out.push(...seg);
      return;
    }
    // Find the final drawn assistant reply in this segment.
    let finalIdx = -1;
    for (let i = seg.length - 1; i >= 0; i--) {
      const r = seg[i];
      if (r.kind === 'item' && r.item.kind === 'agent' && !undrawn(r.item)) {
        finalIdx = i;
        break;
      }
    }
    if (finalIdx <= 0) {
      // No work before a final reply (either no final reply or it's already first).
      if (opts.mode === 'zen' && finalIdx < 0) {
        // In zen mode, a turn with only tools and no final reply still hides non-pinned tools.
        let chunk: ThreadRow[] = [];
        const flushChunk = () => {
          if (chunk.length > 0) out.push(makeTurnFold(chunk, startAt, 0, true));
          chunk = [];
        };
        for (const r of seg) {
          if (isPinnedInTurn(r)) {
            flushChunk();
            out.push(r);
          } else chunk.push(r);
        }
        flushChunk();
        return;
      }
      out.push(...seg);
      return;
    }

    const before = seg.slice(0, finalIdx);
    const finalRow = seg[finalIdx];
    const finalAt = finalRow.kind === 'item' ? finalRow.item.at : 0;
    const after = seg.slice(finalIdx + 1);

    let chunk: ThreadRow[] = [];
    let usedTurnStart = false;
    const flushChunk = (endAt: number) => {
      if (chunk.length === 0) return;
      // Don't wrap only undrawn listening dashes.
      const visible = chunk.some((r) => !(r.kind === 'item' && r.item.kind === 'agent' && undrawn(r.item)));
      if (visible) {
        out.push(makeTurnFold(chunk, usedTurnStart ? 0 : startAt, endAt, hidden));
        usedTurnStart = true;
      } else {
        out.push(...chunk);
      }
      chunk = [];
    };

    for (const r of before) {
      if (isPinnedInTurn(r)) {
        flushChunk(0);
        out.push(r);
      } else {
        chunk.push(r);
      }
    }
    flushChunk(finalAt);
    out.push(finalRow);
    // Trailing tool calls after the final reply (rare): keep pinned/failed, fold or hide others in zen.
    if (after.length > 0) {
      if (opts.mode === 'zen') {
        for (const r of after) {
          if (isPinnedInTurn(r)) out.push(r);
          else out.push(makeTurnFold([r], 0, 0, true));
        }
      } else {
        out.push(...after);
      }
    }
  };

  for (let i = 0; i < rows.length; i++) {
    if (isBoundary(rows[i])) {
      emitSegment(rows.slice(segStart, i), false);
      out.push(rows[i]);
      startAt = boundaryAt(rows[i]);
      segStart = i + 1;
    }
  }
  emitSegment(rows.slice(segStart), true);
  return out;
}

/** Label shown on a collapsed turn fold row: `Worked for 2m · Ran 19 commands`. */
export function turnFoldLabel(row: Extract<ThreadRow, { kind: 'turn' }>): string {
  const head = row.took ? `Worked for ${row.took}` : 'Worked';
  return row.summary ? `${head} · ${row.summary}` : head;
}

/** Whether ⌘F's pattern matches anything inside a folded turn (tool lines or intermediate replies). */
export function turnMatchesFind(rows: ThreadRow[], pattern: RegExp): boolean {
  for (const r of rows) {
    if (r.kind === 'tools') {
      if (r.calls.some((c) => says(pattern, shownText(c.item)))) return true;
    } else if (r.kind === 'item') {
      if (says(pattern, shownText(r.item))) return true;
    } else if (r.kind === 'turn') {
      if (turnMatchesFind(r.rows, pattern)) return true;
    }
  }
  return false;
}
