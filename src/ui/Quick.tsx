// The quick panel's page (quick.rs has its window): one input, and below it what you can do
// next. With nothing typed it lists the sessions that need you, then the ones whose turn ended,
// then a few todos ready to start, then the other sessions started in the last day; typed words find sessions, todos and PRs; the last row starts a
// new session with what you typed as its prompt, in the worktree, on the agent and in the app
// chosen at the bottom.
//
// ⏎ does the selected row and ⌘⏎ always starts. ⌥⏎ shows the session in Codealong, ⌘1 to ⌘9
// open a row, ⌘P picks the project and ⌥⌘1 to ⌥⌘9 switch to one (⌥⌘0 to all of them), ⌘T picks
// the worktree and ⌘L the agent, ↑ in the empty input brings back the last draft and earlier
// prompts, and Escape closes. (⌘M is the Window menu's Minimize, which may take the key before
// the page sees it.)
//
// ⇥ on a session answers it here: the panel shows its conversation and a box that sends it a
// message where it runs, or what it waits on you to allow (ReplyView.tsx). ⏎ sends, and Escape
// goes back to the list, as marking it seen or archiving it from its header does.

import { reuse } from '../reuse';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  archiveSession,
  fitQuick,
  getBoard,
  hideQuick,
  inPreview,
  markSeen,
  onBoard,
  onQuickHidden,
  onQuickOpen,
  openFromBar,
  openUrl,
  quickReady,
  routeQuick,
  searchText,
  sendToSession,
  showMain,
  showQuick,
  startFromQuick,
  startOptions,
  tellOpener,
  todoOp,
  type QuickOpen,
} from '../api';
import { filesToAttach, keepFiles, useFileDrop } from '../attachInput';
import { showContextMenu } from '../contextMenu';
import { ago, agoPhrase, resetsPhrase, tildePath, withTimes } from '../format';
import { HARNESS_WORD, usageNote } from '../harness';
import { useLogo } from '../logo';
import { withoutAccount } from '../lanes';
import { prPills } from '../prStatus';
import { ALL_PROJECTS, attentionRows, findPrs, findSessions, findTodos, FOUND_SESSIONS, inProject, laneBranch, laneGroups, MAX_ROWS, projectCounts, queryWords, readyTodos, recallList, recentRows, shortcutLabel, TEXT_SEARCH_MIN, type PrRow, type RowGroup, type SessionRow, type TodoRow, withPrompt as historyWith } from '../quick';
import {
  accountFor,
  agentId,
  agentOptions,
  autoModel,
  autoPick,
  initialChoice,
  isAuto,
  preferredNames,
  modelName,
  parseAgent,
  placesFor,
  settledPlace,
  remember,
  remembered,
  takesModel,
  type AgentOption,
  type AutoPick,
} from '../start';
import { duplicateOf, rankedBy, suggestions, sureness, todoTitle, worthRouting, type Suggestion } from '../route';
import { attachmentsIn, blockedBy, insertAttachments, newTodo, sessionHomes, snippet, todoItem, todoPrompt, todosPrompt, withoutAttachment } from '../todos';
import type { Attachment, Board, Lane, Meter, PrItem, Route, Session, StartOptions, StartPlace, StartSpec, TextFound } from '../types';
import { AttachmentStrip } from './DescriptionEditor';
import { Picker, type PickerOption, type PickerSection } from './Picker';
import { ReplyView } from './ReplyView';
import { Icon, Kbd, Mark, STATUS_WORD, StatusGlyph } from './primitives';
import { projectForDigit, projectKey } from './ProjectSwitcher';
import { PrGlyph } from './PrsView';
import { applyTheme } from './ThemeSwitch';
import { TodoGlyph } from './TodoBits';
import { UsageMeter } from './UsageBar';

/** Room around the card for its shadow. The window (quick.rs) is the card and this on each side. */
const MARGIN = 20;
const PROJECT_KEY = 'codealong.quick.project';
const HISTORY_KEY = 'codealong.quick.history';
/** The input grows to this many lines, then scrolls. */
const INPUT_LINES = 8;
const COMPOSE = 'compose';
/** How long a read of where accounts can start is used without waiting for a new one. */
const OPTIONS_FOR_MS = 60_000;
/** The tallest the card gets while it answers a session; the conversation scrolls inside. */
const REPLY_HEIGHT = 640;

type PickerKind = 'project' | 'worktree' | 'agent';

const GROUP_WORD: Record<RowGroup, string> = { suggested: 'Suggested', needs: 'Needs you', unread: 'Your turn', working: 'Working', found: 'Sessions', todos: 'Todos', prs: 'PRs', ready: 'Ready todos', recent: 'Last 24 hours' };

/** A suggestion from Jev, as a row. */
interface SuggestRow {
  kind: 'suggest';
  key: string;
  group: 'suggested';
  s: Suggestion;
}

type Row = SessionRow | TodoRow | PrRow | SuggestRow;

/** How long typing has to pause before what's typed goes to Jev. */
const ROUTE_AFTER_MS = 350;
/** How long typing has to pause before what's typed is looked for in what sessions said. */
const TEXT_AFTER_MS = 150;
/** How long typing has to pause before the list may shrink to what it shows. */
const SETTLE_MS = 600;

function plain(e: unknown): string {
  return String(e).replace(/^Error: /, '');
}

function storedHistory(): string[] {
  try {
    const v: unknown = JSON.parse(window.localStorage.getItem(HISTORY_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

function storedProject(): string | null {
  try {
    return window.localStorage.getItem(PROJECT_KEY);
  } catch {
    return null;
  }
}

function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}

/** Why Auto starts the account and model it does. `fallback`: it isn't one of the preferred
 *  accounts, which are all near a limit. */
function autoWhy(p: AutoPick, spent: Meter | null, fallback: boolean, now: number): string {
  if (fallback) return 'preferred accounts are near their limits';
  if (spent) return `${spent.label}’s week is ${Math.round(spent.percent)}% used`;
  if (p.reason === 'reset' && p.resetsAt) return `week resets ${resetsPhrase(p.resetsAt, now)}`;
  return p.reason === 'room' ? 'most room' : '';
}

/** What a session is doing or said last, for its row. Its time is in the row's own column, so
 *  the detail's running time is left out. */
function rowWhy(detail: string | null, headline: string, now: number): string {
  const text = detail ? detail.replace(/^\{for:-?\d+\}( · )?/, '') : withoutAccount(headline);
  return withTimes(text.replace(/`/g, ''), now);
}

/** What a found todo's row says under its title: what it waits for, else the start of its
 *  description, else how many sessions it has. */
function todoWhy(r: TodoRow): string {
  const t = r.item.todo;
  if (t.waiting) return t.waiting.note ? `Waiting for ${t.waiting.note}` : 'Waiting';
  const live = r.item.sessions.find((x) => x.session.live);
  if (live) return `${live.session.account} · ${STATUS_WORD[live.session.status]}`;
  const n = r.item.sessions.length;
  return snippet(t.description, 120) || (n > 0 ? `${n} session${n === 1 ? '' : 's'}` : 'No session yet');
}

/** What a found PR's row says under its title: its number, who opened it, and where it stands.
 *  Checks read as words ("10 checks passed"): the row has no icons to carry them. */
function prWhy(pr: PrItem): string {
  return [`#${pr.number}`, pr.mine ? 'yours' : pr.author, ...prPills(pr).map((p) => (p.key === 'checks' ? p.title : p.text))].filter(Boolean).join(' · ');
}

/** A title with the typed words underlined. */
function Marked({ text, marks }: { text: string; marks: [number, number][] }) {
  if (marks.length === 0) return <>{text}</>;
  const out: ReactNode[] = [];
  let at = 0;
  marks.forEach(([a, b], i) => {
    if (a > at) out.push(text.slice(at, a));
    out.push(<mark key={i}>{text.slice(a, b)}</mark>);
    at = b;
  });
  if (at < text.length) out.push(text.slice(at));
  return <>{out}</>;
}

export function Quick() {
  const [board, setBoard] = useState<Board | null>(null);
  const [options, setOptions] = useState<StartOptions | null>(null);
  // Up (or about to be): the page follows the board only then.
  const [shown, setShown] = useState(false);
  // Counts opens; each one is answered with the page's height once it's laid out.
  const [opens, setOpens] = useState(0);
  // How tall the window can be on its screen; the card keeps within it and the list scrolls.
  const [room, setRoom] = useState<number | null>(null);
  const [opener, setOpener] = useState<string | null>(null);
  const [project, setProject] = useState<string>(() => storedProject() ?? ALL_PROJECTS);
  const [text, setText] = useState('');
  const [laneId, setLaneId] = useState<string | null>(null);
  // The agent menu's choice; null is the first it offers, Auto where there's one.
  const [agent, setAgent] = useState<string | null>(null);
  const [place, setPlace] = useState<StartPlace | null>(null);
  const [newWorktree, setNewWorktree] = useState(false);
  // The todos a new session is for: one, or several started together on one prompt.
  const [todoIds, setTodoIds] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [picker, setPicker] = useState<PickerKind | null>(null);
  // The session the panel answers in place of the list (⇥ on its row).
  const [replyKey, setReplyKey] = useState<string | null>(null);
  const now = useNow(30_000);
  const input = useRef<HTMLTextAreaElement>(null);
  const inputBox = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const textNow = useRef(text);
  textNow.current = text;
  const cachedOptions = useRef<{ at: number; options: StartOptions } | null>(null);
  // Jev's answers by project and text, so going back to what you typed doesn't ask again. Each
  // open starts afresh: sessions come and go.
  const routes = useRef(new Map<string, Route>());
  // What you'd typed when you last closed the panel without acting on it. Every open starts empty,
  // so the list is what needs you and ⏎ goes to it; ↑ in the empty input brings the draft back.
  const [draft, setDraft] = useState<{ text: string; todos: string[] } | null>(null);
  // Prompts you sent, started or saved, newest first, kept across restarts. ↑ walks back through
  // the draft and then these; `recall` is where it is, and holds while the text is what it
  // brought back.
  const [history, setHistory] = useState<string[]>(storedHistory);
  const [recall, setRecall] = useState<number | null>(null);
  const keepPrompt = (prompt: string) => {
    setHistory((h) => {
      const next = historyWith(h, prompt);
      try {
        window.localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
      } catch {
        // Storage can be unavailable; the prompt just won't come back after a restart.
      }
      return next;
    });
  };
  const todoIdsNow = useRef(todoIds);
  todoIdsNow.current = todoIds;

  // Opening: what for, then fresh data, then the height (below), and only then the panel shows.
  useEffect(() => {
    const open = (ctx: QuickOpen) => {
      setPicker(null);
      setRoom(ctx.room ?? null);
      if (!ctx.keep) {
        routes.current.clear();
        setOpener(ctx.opener ?? null);
        setError(null);
        setProject(ctx.project ?? storedProject() ?? ALL_PROJECTS);
        setLaneId(ctx.laneId ?? null);
        setTodoIds(ctx.todos ?? (ctx.todo ? [ctx.todo] : []));
        if (textNow.current.trim()) setDraft({ text: textNow.current, todos: todoIdsNow.current });
        setText(ctx.prompt ?? '');
        setRecall(null);
        setAgent(null);
        setPlace(null);
        setNewWorktree(false);
        setActiveKey(null);
        setReplyKey(ctx.reply ?? null);
      }
      // Where accounts can start rarely changes, and reading it means reading Codex's state
      // file, so a recent copy does while a fresh one is read.
      const fresh = startOptions().then((o) => {
        cachedOptions.current = { at: Date.now(), options: o };
        setOptions(o);
        return o;
      });
      const recent = cachedOptions.current && Date.now() - cachedOptions.current.at < OPTIONS_FOR_MS ? cachedOptions.current.options : null;
      // Not waited on, a failed read just leaves the recent copy.
      if (recent) fresh.catch(() => {});
      Promise.all([getBoard(), recent ?? fresh])
        .then(([b, o]) => {
          setBoard(b);
          setOptions(o);
          // A todo asked for without its prompt (by the floating bar, which doesn't have it)
          // starts the way the Todo view starts it: as its prompt, in its worktree.
          const ids = ctx.todos ?? (ctx.todo ? [ctx.todo] : []);
          const ts = !ctx.keep && ctx.prompt === undefined ? ids.flatMap((id) => b.todos.find((x) => x.id === id) ?? []) : [];
          if (ts.length > 0) {
            setText(todosPrompt(ts));
            const laneById = new Map(b.lanes.map((l) => [l.id, l]));
            const homes = sessionHomes(b.lanes);
            if (!ctx.laneId) setLaneId(ts.map((t) => todoItem(t, laneById, homes, new Map()).lane?.id).find(Boolean) ?? null);
          }
        })
        .catch((e: unknown) => setError(plain(e)))
        .finally(() => {
          setShown(true);
          setOpens((n) => n + 1);
        });
    };
    const offOpen = onQuickOpen(open);
    const offHidden = onQuickHidden(() => {
      setShown(false);
      setPicker(null);
    });
    // A browser preview has no engine to open it.
    if (inPreview) open({});
    return () => {
      offOpen();
      offHidden();
    };
  }, []);

  useEffect(() => (shown ? onBoard((b) => setBoard((prev) => (prev ? reuse(prev, b) : b))) : undefined), [shown]);

  const theme = board?.theme ?? 'system';
  useEffect(() => applyTheme(theme), [theme]);
  const logoStatus = useLogo(board?.logoStatus);

  // The input grows with what you write.
  useLayoutEffect(() => {
    const el = input.current;
    if (!el) return;
    el.style.height = 'auto';
    const line = parseFloat(getComputedStyle(el).lineHeight) || 22;
    el.style.height = `${Math.min(el.scrollHeight, Math.ceil(line * INPUT_LINES))}px`;
  }, [text, opens]);

  // The window is as tall as the card, or an open menu where it reaches below the card.
  const measure = useCallback(() => {
    const c = card.current?.getBoundingClientRect();
    const pop = document.querySelector('.ca-picker')?.getBoundingClientRect();
    return Math.ceil(Math.max(c?.bottom ?? 0, pop?.bottom ?? 0) + MARGIN);
  }, []);
  const refocus = () => window.requestAnimationFrame(() => input.current?.focus());

  /* ---- Where ---- */

  const projects = useMemo(() => board?.projects ?? [], [board?.projects]);
  const current = project === ALL_PROJECTS || projects.some((p) => p.name === project) ? project : ALL_PROJECTS;
  const pickProject = (name: string) => {
    setProject(name);
    setActiveKey(null);
    setPicker(null);
    try {
      window.localStorage.setItem(PROJECT_KEY, name);
    } catch {
      // Storage can be unavailable; the choice just won't be remembered.
    }
    refocus();
  };
  const counts = useMemo(() => (board ? projectCounts(board, [...projects.map((p) => p.name), ALL_PROJECTS]) : {}), [board, projects]);

  // Where a session can start: the project's worktrees, and the one a window asked for.
  const lanes = useMemo(() => {
    const all = board?.lanes ?? [];
    const mine = all.filter((l) => inProject(l, current));
    const extra = laneId && !mine.some((l) => l.id === laneId) ? all.filter((l) => l.id === laneId) : [];
    return [...mine, ...extra];
  }, [board?.lanes, current, laneId]);
  const groups = useMemo(() => laneGroups(lanes, board?.pinnedWorktrees ?? []), [lanes, board?.pinnedWorktrees]);
  // Without one asked for, the first on the list: a pinned one, else the latest.
  const lane = lanes.find((l) => l.id === laneId) ?? groups[0]?.lanes[0] ?? null;

  // The session answered in place of the list, while it's on the board.
  const replying = useMemo(() => {
    if (!board || !replyKey) return null;
    for (const l of board.lanes) {
      const session = l.sessions.find((x) => x.key === replyKey);
      if (session) return { lane: l, session };
    }
    return null;
  }, [board, replyKey]);

  /* ---- Who ---- */

  const usage = useMemo(() => board?.usage ?? [], [board?.usage]);
  // Stars come with the board, which is current; where accounts can start is read less often.
  const accounts = useMemo(() => (options?.accounts ?? []).map((a) => ({ ...a, preferred: usage.find((u) => u.account === a.name)?.preferred ?? a.preferred })), [options, usage]);
  const models = useMemo(() => options?.claudeModels ?? [], [options]);
  // Auto follows usage while the panel is up, and resolves when you start.
  const pick = useMemo(() => autoPick(accounts, usage, now), [accounts, usage, now]);
  const agents = useMemo(() => agentOptions(accounts, models, pick), [accounts, models, pick]);
  const first = useMemo(() => initialChoice(accounts, pick, undefined), [accounts, pick]);
  const agentNow = agent && agents.some((o) => o.id === agent) ? agent : first ? agentId(first, null) : null;
  const { choice, model: chosenModel } = agentNow ? parseAgent(agentNow) : { choice: null, model: null };
  const autoFor = isAuto(choice);
  const account = accountFor(choice, pick);
  const accountInfo = accounts.find((a) => a.name === account);
  const harness = accountInfo?.harness ?? null;
  const autoModelPick = autoFor ? autoModel(accountInfo?.defaultModel ?? null, usage.find((u) => u.account === account)) : null;

  // A model is asked for on the command line, so choosing one leaves only the terminal and the
  // background; the apps pick their own.
  const allPlaces = placesFor(choice, accounts, pick);
  const places = chosenModel ? allPlaces.filter((p) => onCommandLine(p.place)) : allPlaces;
  const placeNow = [place, choice ? (settledPlace(choice, accounts, pick) ?? remembered().places?.[choice]) : undefined].map((p) => places.find((x) => x.place === p)?.place).find(Boolean) ?? places[0]?.place ?? null;
  const claudeTerminal = !!placeNow && onCommandLine(placeNow) && harness === 'claude';
  const modelNow = claudeTerminal ? (autoFor ? (autoModelPick?.model ?? null) : chosenModel) : null;
  const inNewWorktree = claudeTerminal && newWorktree;
  const defaultModel = accountInfo?.defaultModel ?? null;
  const modelShown = !claudeTerminal
    ? placeNow === 'claude-app'
      ? 'model chosen in the Claude app'
      : placeNow === 'codex-app'
        ? 'model chosen in Codex'
        : harness === 'opencode'
          ? 'OpenCode’s model'
          : harness === 'codex'
            ? 'Codex’s model'
            : null
    : modelNow
      ? modelName(modelNow, models)
      : defaultModel
        ? modelName(defaultModel, models)
        : 'default model';

  /* ---- What for ---- */

  const forTodos = useMemo(() => (board ? todoIds.flatMap((id) => board.todos.find((t) => t.id === id) ?? []) : []), [board, todoIds]);
  const todo = forTodos[0] ?? null;
  // What each waits on that isn't done, leaving out the others: started together, they're done
  // together.
  const blocked = board ? forTodos.map((t) => ({ t, on: blockedBy(t, board.todos).filter((w) => !todoIds.includes(w.id)) })).filter((b) => b.on.length > 0) : [];
  const waitsOn = [...new Map(blocked.flatMap((b) => b.on).map((w) => [w.id, w])).values()];
  const marked = forTodos.filter((t) => t.waiting);
  const attached = attachmentsIn(text);

  // Attachments go in at the cursor, each on a line of its own.
  const addAttachments = (kept: Attachment[]) => {
    const el = input.current;
    const t = textNow.current;
    const r = insertAttachments(t, el && document.activeElement === el ? el.selectionStart : t.length, kept);
    setText(r.text);
    window.requestAnimationFrame(() => {
      input.current?.focus();
      input.current?.setSelectionRange(r.cursor, r.cursor);
    });
  };
  useFileDrop(inputBox, addAttachments, setError);

  /* ---- The list ---- */

  // With a todo to start, the input holds its prompt, not a search: only the new session is
  // listed.
  const prompted = text.trim().length > 0;
  const query = todo ? '' : text.trim();
  // With nothing typed, a few todos ready to start go under the sessions, which leave them room.
  const readyList = useMemo(() => (board && !query && !todo ? readyTodos(board, current) : null), [board, current, query, todo]);
  const attention = useMemo(() => (board && !query && !todo ? attentionRows(board, current, MAX_ROWS - (readyList?.rows.length ?? 0)) : null), [board, current, query, todo, readyList]);
  // Below them, the rest of the day's sessions, for ↓ to reach one that isn't waiting on you.
  const recent = useMemo(() => (board && attention ? recentRows(board, current, new Set(attention.rows.map((r) => r.key)), now) : null), [board, current, attention, now]);
  // What was said in sessions is searched in the engine, once typing pauses; its hits join the
  // list below the sessions whose titles and the like match.
  const textFor = !!board && query.replace(/\s+/g, '').length >= TEXT_SEARCH_MIN ? queryWords(query).join(' ') : null;
  const [textSearch, setTextSearch] = useState<{ for: string; found: TextFound } | null>(null);
  useEffect(() => {
    if (!textFor) return;
    let live = true;
    const id = window.setTimeout(() => {
      searchText(textFor.split(' '))
        .then((found) => live && setTextSearch({ for: textFor, found }))
        .catch(() => undefined);
    }, TEXT_AFTER_MS);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [textFor]);
  const textFound = textFor && textSearch?.for === textFor ? textSearch.found : null;
  const said = useMemo(() => new Map((textFound?.hits ?? []).map((h) => [h.key, h] as const)), [textFound]);
  const foundSessions = useMemo(() => (board && query ? findSessions(board, current, query, FOUND_SESSIONS, said) : []), [board, current, query, said]);
  const foundTodos = useMemo(() => (board && query ? findTodos(board, current, query) : []), [board, current, query]);
  const foundPrs = useMemo(() => (board && query ? findPrs(board, current, query) : []), [board, current, query]);

  // With routing on, what you type goes to Jev once you pause, and its suggestions lead the
  // list, so ⏎ does the likeliest. A word or two is a search and isn't sent.
  const routeFor = board?.jev?.quick_panel && !todo && worthRouting(query) ? `${current}\n${query}` : null;
  const [routed, setRouted] = useState<{ for: string; route: Route } | null>(null);
  const [routeError, setRouteError] = useState<{ for: string; message: string } | null>(null);
  useEffect(() => {
    if (!routeFor) return;
    const cached = routes.current.get(routeFor);
    if (cached) {
      setRouted({ for: routeFor, route: cached });
      return;
    }
    let live = true;
    const at = routeFor.indexOf('\n');
    const id = window.setTimeout(() => {
      routeQuick(routeFor.slice(0, at), routeFor.slice(at + 1))
        .then((r) => {
          routes.current.set(routeFor, r);
          if (live) setRouted({ for: routeFor, route: r });
        })
        .catch((e: unknown) => live && setRouteError({ for: routeFor, message: plain(e) }));
    }, ROUTE_AFTER_MS);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [routeFor]);
  const route = routeFor && routed?.for === routeFor ? routed.route : null;
  const routeProblem = routeFor && routeError?.for === routeFor ? routeError.message : null;
  const routing = !!routeFor && !route && !routeProblem;
  const suggested = useMemo<SuggestRow[]>(() => (board && route ? suggestions(route, board).map((sg) => ({ kind: 'suggest', key: sg.key, group: 'suggested', s: sg })) : []), [board, route]);

  // What Jev said beyond where it goes: a session that may already be on it. Jev picks where
  // work goes, never the model: that stays with the start menu and Auto.
  const duplicate = board ? duplicateOf(route, board) : null;
  const alreadyOn = duplicate ? ` “${duplicate.session.title}” (${duplicate.session.account} · ${duplicate.lane.callsign}) may already be doing this.` : '';

  // While you type, the list only grows: Jev's suggestions and what sessions said arrive a moment
  // after each key, and letting the list shrink in between made the panel bounce with every key.
  // Once typing pauses and the answers are in, it takes the height of what it shows.
  const [paused, setPaused] = useState(true);
  useEffect(() => {
    setPaused(false);
    const id = window.setTimeout(() => setPaused(true), SETTLE_MS);
    return () => window.clearTimeout(id);
  }, [text]);
  const settled = paused && !routing && (!textFor || !!textFound);
  const held = useRef({ open: 0, height: 0 });
  useLayoutEffect(() => {
    const el = list.current;
    if (!el) return;
    el.style.minHeight = '';
    if (!text.trim() || settled || held.current.open !== opens) held.current = { open: opens, height: 0 };
    if (!text.trim() || settled) return;
    held.current.height = Math.max(held.current.height, el.offsetHeight);
    // Only as far as the screen has room: past that the list scrolls, as it does once settled.
    const spare = room && card.current ? room - 2 * MARGIN - card.current.offsetHeight : Infinity;
    el.style.minHeight = `${Math.min(held.current.height, el.offsetHeight + spare)}px`;
  });
  // A scrollbar, where macOS shows one, narrows the rows while the list scrolls; the new-session
  // row below it keeps in line with them.
  useLayoutEffect(() => {
    const el = scroller.current;
    list.current?.style.setProperty('--quick-gutter', `${el ? el.offsetWidth - el.clientWidth : 0}px`);
  });

  const reported = useRef(0);
  useLayoutEffect(() => {
    if (opens === 0) return;
    const h = measure();
    reported.current = h;
    // The cursor goes at the end of whatever the panel opens with: nothing, a todo's prompt, or
    // what a failed start left.
    const el = input.current;
    if (el) {
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    }
    void quickReady(h);
  }, [opens, measure]);
  useLayoutEffect(() => {
    if (!shown || opens === 0) return;
    const h = measure();
    if (h === reported.current) return;
    reported.current = h;
    void fitQuick(h);
  });
  // Answering a session, the conversation loads and grows on its own, without the panel drawing
  // again: the window follows the card.
  const replyingKey = replying?.session.key ?? null;
  useLayoutEffect(() => {
    const el = card.current;
    if (!replyingKey || !el || !shown || opens === 0) return;
    const observer = new ResizeObserver(() => {
      const h = measure();
      if (h === reported.current) return;
      reported.current = h;
      void fitQuick(h);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [replyingKey, shown, opens, measure]);

  const rows: Row[] = query ? [...suggested, ...rankedBy(foundSessions, route), ...foundTodos, ...foundPrs] : [...(attention?.rows ?? []), ...(readyList?.rows ?? []), ...(recent?.rows ?? [])];
  const keys = [...rows.map((r) => r.key), COMPOSE];
  const recallable = useMemo(() => recallList(draft?.text ?? null, history), [draft, history]);
  const active = activeKey && keys.includes(activeKey) ? activeKey : keys[0];

  useEffect(() => {
    list.current?.querySelector('.is-active')?.scrollIntoView({ block: 'nearest' });
  }, [active]);

  const move = (delta: number) => {
    const i = keys.indexOf(active);
    setActiveKey(keys[Math.max(0, Math.min(keys.length - 1, i + delta))]);
  };

  /** Opens a session where it lives. The search that found it is done with. */
  const openSession = (lane: Lane, session: Session) => {
    const target = lane.targets.find((t) => t.id === session.openTarget);
    if (target?.disabled) {
      setError(target.detail ?? `${target.label} isn’t available right now.`);
      return;
    }
    const kept = textNow.current;
    setText('');
    setActiveKey(null);
    setError(null);
    void hideQuick();
    openFromBar(lane.id, session.openTarget).catch((e: unknown) => {
      setText(kept);
      setError(plain(e));
      void showQuick({ keep: true });
    });
  };

  const revealSession = (lane: Lane, session: Session) => {
    void hideQuick();
    void showMain({ kind: 'session', laneId: lane.id, key: session.key });
  };

  /** Answers a session here: its conversation, and a box for the reply. What's typed in the input
   *  stays for when you go back. */
  const replyTo = (session: Session) => {
    setReplyKey(session.key);
    setPicker(null);
    setError(null);
  };

  const leaveReply = () => {
    setReplyKey(null);
    refocus();
  };

  /** Marks the session answered seen or archives it, and goes back to the list it's left. */
  const dismissReply = (act: Promise<void>) => {
    leaveReply();
    act.catch((e: unknown) => setError(plain(e)));
  };

  /** The session ⏎ opens for a todo, as in the main window: the live one, else the latest. */
  const todoSession = (r: TodoRow) => r.item.sessions.find((x) => x.session.live) ?? r.item.sessions[0] ?? null;

  /** Makes the new session the todo's: its prompt, its worktree, and linked to it. It starts
   *  when you say so. */
  const prepareTodo = (r: TodoRow) => {
    setTodoIds([r.item.todo.id]);
    setText(todoPrompt(r.item.todo));
    if (r.item.lane) setLaneId(r.item.lane.id);
    setActiveKey(COMPOSE);
    setError(null);
    window.requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
      el.scrollTop = 0;
    });
  };

  const openTodo = (r: TodoRow) => {
    const s = todoSession(r);
    if (s) openSession(s.lane, s.session);
    else prepareTodo(r);
  };

  const revealTodo = (r: TodoRow) => {
    void hideQuick();
    void showMain({ kind: 'todo', id: r.item.todo.id });
  };

  /** A PR opens on GitHub, as it does in the PRs view. */
  const openPr = (pr: PrItem) => {
    void hideQuick();
    openUrl(pr.url).catch((e: unknown) => {
      setError(plain(e));
      void showQuick({ keep: true });
    });
  };

  const revealPr = (pr: PrItem) => {
    void hideQuick();
    void showMain({ kind: 'pr', key: pr.key });
  };

  /** The latest session that worked on a PR, where it's listed. */
  const prSession = (pr: PrItem): { lane: Lane; session: Session } | null => {
    const found = pr.sessions.flatMap((x) => {
      const lane = board?.lanes.find((l) => l.id === x.lane);
      const session = lane?.sessions.find((s) => s.key === x.key);
      return lane && session ? [{ lane, session }] : [];
    });
    return found.sort((a, b) => Number(b.session.live) - Number(a.session.live) || b.session.lastActivity - a.session.lastActivity)[0] ?? null;
  };

  const rowMenu = (r: Row) => {
    if (r.kind === 'suggest') {
      const sg = r.s;
      if (sg.kind === 'send' || sg.kind === 'open') {
        void showContextMenu([
          ...(sg.kind === 'send' ? [{ label: 'Send this prompt to it', run: () => send(sg.session) }] : []),
          { label: 'Open', run: () => openSession(sg.lane, sg.session) },
          { label: 'Reply here', run: () => replyTo(sg.session) },
          { label: 'Show in Codealong', run: () => revealSession(sg.lane, sg.session) },
        ]);
      } else {
        void showContextMenu([
          { label: `Start a session in ${sg.lane.callsign}`, run: () => start({ lane: sg.lane, newWorktree: sg.kind === 'start' && sg.newWorktree }) },
          { label: `Save as a todo in ${sg.lane.project}`, run: () => saveTodo(sg.lane) },
        ]);
      }
      return;
    }
    if (r.kind === 'session') {
      void showContextMenu([
        { label: 'Open', run: () => openSession(r.lane, r.session) },
        { label: 'Reply here', run: () => replyTo(r.session) },
        { label: 'Show in Codealong', run: () => revealSession(r.lane, r.session) },
      ]);
      return;
    }
    if (r.kind === 'pr') {
      const s = prSession(r.pr);
      void showContextMenu([
        { label: 'Open on GitHub', run: () => openPr(r.pr) },
        ...(s ? [{ label: s.session.live ? 'Open its session' : 'Open its latest session', run: () => openSession(s.lane, s.session) }] : []),
        { label: 'Show in Codealong', run: () => revealPr(r.pr) },
      ]);
      return;
    }
    const s = todoSession(r);
    void showContextMenu([
      ...(s ? [{ label: s.session.live ? 'Open its session' : 'Open its latest session', run: () => openSession(s.lane, s.session) }] : []),
      { label: 'Start a session for it', run: () => prepareTodo(r) },
      { label: 'Show in Codealong', run: () => revealTodo(r) },
    ]);
  };

  /* ---- Starting ---- */

  const ready = !!lane && !!account && !!placeNow;
  const terminalName = places.find((p) => p.place === 'terminal')?.label ?? 'The terminal';
  // Claude Code starts on the model asked for, else the account's own.
  const startsOn = claudeTerminal ? (modelNow ?? defaultModel) : null;
  const onModel = startsOn ? ` on ${modelName(startsOn, models)}` : '';
  const withPrompt = prompted ? ' with this prompt' : '';
  const happens = !lane
    ? 'No worktrees in this project yet.'
    : !account || !placeNow
      ? ''
      : placeNow === 'background'
        ? inNewWorktree
          ? `Claude Code makes a new worktree of ${lane.repo} and starts ${account}${onModel} there in the background${withPrompt}. Attach a terminal any time.`
          : `${account}${onModel} starts in the background in ${lane.callsign}${withPrompt}, with no terminal. Attach one any time.`
        : placeNow === 'terminal'
        ? inNewWorktree
          ? `Claude Code makes a new worktree of ${lane.repo} and starts ${account}${onModel} there in ${terminalName}${withPrompt}.`
          : `${terminalName} starts ${account}${onModel} in ${lane.callsign}${withPrompt}.`
        : placeNow === 'claude-app'
          ? `The Claude app opens a new session in ${lane.callsign}${prompted ? ' with this prompt filled in' : ''}. You send it from there.`
          : `Codex opens a new thread in ${lane.callsign}${prompted ? ' with this prompt filled in' : ''}. You send it from there.`;
  const newToCodex = placeNow === 'codex-app' && !!lane && !!options && !options.codexProjects.includes(lane.path);

  /** Starts a session: in the worktree chosen below, or where a suggestion says. */
  const start = (over?: { lane: Lane; newWorktree: boolean }) => {
    const where = over?.lane ?? lane;
    const newTree = claudeTerminal && (over ? over.newWorktree : newWorktree);
    if (!where || !account || !placeNow) return;
    if (choice) remember(choice, placeNow);
    const spec: StartSpec = { laneId: where.id, account, place: placeNow, prompt: text, model: modelNow, worktree: newTree, todos: forTodos.map((t) => t.id) };
    const label =
      placeNow === 'terminal' || placeNow === 'background'
        ? `Starting ${account}${onModel} in ${newTree ? `a new worktree of ${where.repo}` : where.callsign}${placeNow === 'background' ? ' in the background' : ''}…`
        : placeNow === 'claude-app'
          ? 'Opening a new session in the Claude app…'
          : 'Opening a new thread in Codex…';
    const id = Date.now();
    const to = opener ?? 'main';
    // The panel is fresh for next time; a failed start brings back what you had.
    const kept = { text, todoIds, newWorktree };
    keepPrompt(text);
    setText('');
    setTodoIds([]);
    setNewWorktree(false);
    setError(null);
    setActiveKey(null);
    void tellOpener(to, { phase: 'begin', id, label });
    void hideQuick();
    startFromQuick(spec)
      .then((started) => tellOpener(to, { phase: 'end', id, key: started?.key ?? null, message: started?.message ?? null }))
      .catch((e: unknown) => {
        void tellOpener(to, { phase: 'end', id, key: null, message: null });
        setText(kept.text);
        setTodoIds(kept.todoIds);
        setNewWorktree(kept.newWorktree);
        setError(plain(e));
        void showQuick({ keep: true });
      });
  };

  /** Sends what's typed to a session that's already going. The panel closes at once; a failed
   *  send brings it back with what you wrote and why. */
  const send = (session: Session) => {
    const prompt = textNow.current;
    const id = Date.now();
    const to = opener ?? 'main';
    keepPrompt(prompt);
    setText('');
    setActiveKey(null);
    setError(null);
    void tellOpener(to, { phase: 'begin', id, label: `Sending to ${session.title}…` });
    void hideQuick();
    sendToSession(session.key, prompt)
      .then((sent) => tellOpener(to, { phase: 'end', id, key: null, message: sent.message }))
      .catch((e: unknown) => {
        void tellOpener(to, { phase: 'end', id, key: null, message: null });
        setText(prompt);
        setError(plain(e));
        void showQuick({ keep: true });
      });
  };

  /** Keeps what's typed as a todo, linked to the worktree. */
  const saveTodo = (where: Lane) => {
    const prompt = textNow.current.trim();
    const title = todoTitle(prompt);
    const t = newTodo({ title, description: prompt === title ? '' : prompt, project: where.project, worktrees: [where.id] }, Date.now());
    const to = opener ?? 'main';
    keepPrompt(prompt);
    setText('');
    setActiveKey(null);
    setError(null);
    void hideQuick();
    todoOp({ op: 'create', todo: t })
      .then(() => tellOpener(to, { phase: 'end', id: Date.now(), key: null, message: `Saved “${title}” as a todo in ${where.project}.` }))
      .catch((e: unknown) => {
        setText(prompt);
        setError(plain(e));
        void showQuick({ keep: true });
      });
  };

  const suggest = (sg: Suggestion, alt: boolean) => {
    if ((sg.kind === 'send' || sg.kind === 'open') && alt) revealSession(sg.lane, sg.session);
    else if (sg.kind === 'send') send(sg.session);
    else if (sg.kind === 'open') openSession(sg.lane, sg.session);
    else if (sg.kind === 'start') start({ lane: sg.lane, newWorktree: sg.newWorktree });
    else saveTodo(sg.lane);
  };

  const activate = (key: string, alt: boolean) => {
    const row = rows.find((r) => r.key === key);
    if (!row) start();
    else if (row.kind === 'suggest') suggest(row.s, alt);
    else if (row.kind === 'todo') (alt ? revealTodo : openTodo)(row);
    else if (row.kind === 'pr') (alt ? revealPr : openPr)(row.pr);
    else (alt ? revealSession : openSession)(row.lane, row.session);
  };

  /* ---- Keys ---- */

  const togglePicker = (k: PickerKind) => setPicker((p) => (p === k ? null : k));

  const onInputKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    const el = e.currentTarget;
    // ⇥ on a session answers it here; elsewhere it moves the focus as usual.
    if (e.key === 'Tab' && !e.shiftKey && !e.metaKey && !e.altKey && !e.ctrlKey) {
      const row = rows.find((r) => r.key === active);
      const session = row?.kind === 'session' ? row.session : row?.kind === 'suggest' && (row.s.kind === 'send' || row.s.kind === 'open') ? row.s.session : null;
      if (session) {
        e.preventDefault();
        replyTo(session);
        return;
      }
    }
    if (e.key === 'Enter') {
      if (e.shiftKey) return;
      e.preventDefault();
      if (e.metaKey) start();
      else activate(active, e.altKey);
      return;
    }
    // ↑ in the empty input brings back the draft, then earlier prompts, and ↓ goes forward again
    // to the empty input; once you change what came back, the arrows move through the list.
    const recalled = recall !== null && el.value === recallable[recall];
    const plainKey = !e.altKey && !e.metaKey && !e.shiftKey && !e.ctrlKey;
    const walk = (e.key === 'ArrowUp' && (recalled || (!el.value && keys.indexOf(active) <= 0))) || (e.key === 'ArrowDown' && recalled);
    if (plainKey && walk) {
      const at = recalled ? recall + (e.key === 'ArrowUp' ? 1 : -1) : 0;
      if (at >= recallable.length) return e.preventDefault();
      e.preventDefault();
      const next = at < 0 ? '' : recallable[at];
      setRecall(at < 0 ? null : at);
      setText(next);
      setTodoIds(at === 0 && draft && recallable[0] === draft.text ? draft.todos : []);
      setActiveKey(null);
      window.requestAnimationFrame(() => el.setSelectionRange(next.length, next.length));
      return;
    }
    if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && !e.altKey && !e.metaKey && !e.shiftKey) {
      // In a prompt of several lines the arrows move the cursor, and the selection only from
      // its first or last line.
      const before = el.value.slice(0, el.selectionStart);
      const after = el.value.slice(el.selectionEnd);
      if (e.key === 'ArrowUp' && before.includes('\n')) return;
      if (e.key === 'ArrowDown' && after.includes('\n')) return;
      e.preventDefault();
      move(e.key === 'ArrowDown' ? 1 : -1);
      return;
    }
    if (e.ctrlKey && !e.metaKey && (e.key === 'n' || e.key === 'p')) {
      e.preventDefault();
      move(e.key === 'n' ? 1 : -1);
    }
  };

  // The panel's own keys, wherever the focus is in it. A menu's filter handles its own first.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        if (picker) {
          setPicker(null);
          refocus();
        } else if (replying) leaveReply();
        else void hideQuick();
        return;
      }
      // Answering a session, only ⌘Q and ⌘H mean something here: the list's keys are for the list.
      if (replying) {
        const k = e.key.toLowerCase();
        if (e.metaKey && !e.ctrlKey && !e.altKey && (k === 'q' || k === 'h')) {
          e.preventDefault();
          void hideQuick();
        }
        return;
      }
      // ⌥⌘ digits switch projects, as in the main window. ⌥ changes the character, so they go by
      // the key's code.
      const digit = /^Digit(\d)$/.exec(e.code);
      if (e.metaKey && e.altKey && !e.ctrlKey && !e.shiftKey && digit) {
        e.preventDefault();
        const name = projectForDigit(Number(digit[1]), projects);
        if (name) pickProject(name);
        return;
      }
      if (!e.metaKey || e.ctrlKey || e.altKey) return;
      if (/^[1-9]$/.test(e.key)) {
        const row = rows[Number(e.key) - 1];
        if (row) {
          e.preventDefault();
          activate(row.key, false);
        }
        return;
      }
      const k = e.key.toLowerCase();
      if (k === 'p' || k === 't' || k === 'l') {
        e.preventDefault();
        togglePicker(k === 'p' ? 'project' : k === 't' ? 'worktree' : 'agent');
      } else if (k === 'q' || k === 'h') {
        // The menu bar is the other app's while the panel is up, so ⌘Q and ⌘H mean that app,
        // not Codealong: they close the panel instead of quitting or hiding Codealong.
        e.preventDefault();
        void hideQuick();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        start();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  /* ---- Menus ---- */

  const projectOption = (name: string): PickerOption => ({
    id: name,
    text: name,
    label: name,
    lead: <Icon name={name === ALL_PROJECTS ? 'stack' : 'folder'} size={14} />,
    trail: (
      <>
        <ProjectCounts needs={counts[name]?.needs ?? 0} unread={counts[name]?.unread ?? 0} />
        <span className="ca-projswitch-key">{projectKey(name, projects)}</span>
      </>
    ),
  });
  // As in the main window's switcher: the projects, then All projects set apart.
  const projectSections: PickerSection[] = [
    { key: 'projects', options: projects.map((p) => projectOption(p.name)) },
    { key: 'all', head: <span className="ca-picker-sep" />, options: [projectOption(ALL_PROJECTS)] },
  ];

  const worktreeSections: PickerSection[] = groups.map((g) => ({
    key: g.label,
    head: g.label,
    options: g.lanes.map((l) => {
      const branch = laneBranch(l);
      return {
        id: l.id,
        text: [l.callsign, l.branch ?? '', l.repo, l.project, l.origin === 'main' ? 'main checkout' : ''].join(' '),
        label: l.origin === 'main' ? `${l.callsign} (main checkout)` : l.callsign,
        detail: branch,
        lead: <StatusGlyph status={l.glyph} size={10} title="" />,
      };
    }),
  }));

  const agentSections: PickerSection[] = useMemo(() => {
    const out: PickerSection[] = [];
    const auto = agents.find((o) => isAuto(o.choice));
    if (auto && pick) {
      const a = accounts.find((x) => x.name === pick.account);
      const m = autoModel(a?.defaultModel ?? null, usage.find((u) => u.account === pick.account));
      const model = m.model ?? a?.defaultModel ?? null;
      const why = autoWhy(pick, m.spent, !preferredNames(accounts).includes(pick.account), now);
      out.push({
        key: 'auto',
        options: [
          {
            id: auto.id,
            text: `auto ${pick.account} ${a ? HARNESS_WORD[a.harness] : ''} ${model ?? ''}`,
            label: 'Auto',
            detail: [pick.account, model ? modelName(model, models) : null, why].filter(Boolean).join(' · '),
            lead: <Icon name="sparkles" size={14} />,
            title:
              `Starts a preferred account (starred in Settings › Accounts) while one has room, else another. Among them, the one whose weekly limit resets soonest, so its room isn't lost, ` +
              `else the one whose fullest limit is least full, on its own default model; when that model's own limit is nearly used, on the next best. ` +
              `An account at 90% of a limit has no room. Decided when you start.`,
          },
        ],
      });
    }
    for (const a of accounts) {
      const u = usage.find((x) => x.account === a.name);
      const mine = agents.filter((o) => !isAuto(o.choice) && o.choice === a.name);
      const cant = a.places.length === 0;
      out.push({
        key: a.name,
        head: <AccountHead name={a.name} detail={[HARNESS_WORD[a.harness], u?.plan, a.preferred ? 'Preferred' : null].filter(Boolean).join(' · ')} meters={cant ? null : (u?.meters ?? [])} note={cant ? 'Can’t start from here' : usageNote(u)} now={now} />,
        options: mine.map((o: AgentOption) => ({
          id: o.id,
          lead: <span className="ca-picker-check">{o.id === agentNow ? <Icon name="check" size={14} /> : null}</span>,
          text: `${a.name} ${HARNESS_WORD[a.harness]} ${o.model ? `${o.model} ${modelName(o.model, models)}` : 'default'}`,
          label: o.model ? modelName(o.model, models) : takesModel(a) ? `Default${a.defaultModel ? ` · ${modelName(a.defaultModel, models)}` : ''}` : a.name,
          detail: o.model ? (models.find((m) => m.id === o.model)?.label.split(': ')[1] ?? null) : takesModel(a) ? 'From its settings.json' : cant ? a.unavailable : a.harness === 'codex' ? 'The model is chosen in Codex' : a.harness === 'opencode' ? 'The model comes from OpenCode’s settings' : null,
          disabled: cant,
          title: cant ? (a.unavailable ?? undefined) : undefined,
        })),
      });
    }
    return out;
  }, [agents, accounts, pick, usage, models, now, agentNow]);

  const pickAgent = (id: string) => {
    const { choice: c, model } = parseAgent(id);
    setAgent(id);
    // A model is asked for on the command line; an app picks its own.
    if (model) setPlace('terminal');
    else setPlace(c ? (settledPlace(c, accounts, pick) ?? remembered().places?.[c] ?? null) : null);
    setPicker(null);
    refocus();
  };

  const pickPlace = (p: StartPlace) => {
    setPlace(p);
    // The apps choose their model: an account on a model becomes the account on its default.
    if (p !== 'terminal' && choice && chosenModel) setAgent(agentId(choice, null));
  };

  /** A suggestion's row: what ⏎ does, and where. */
  const describe = (sg: Suggestion): { icon: string; title: string; why: string; where: string } => {
    if (sg.kind === 'send' || sg.kind === 'open') {
      const x = sg.session;
      const where = `${x.account} · ${sg.lane.callsign}`;
      // How old it is leads, so an old session isn't mistaken for the one you started this morning.
      const age = x.startedAt != null ? `Started ${agoPhrase(x.startedAt, now)}. ` : '';
      if (sg.kind === 'open') {
        return x.status === 'needs'
          ? { icon: 'external', title: `Answer “${x.title}”`, why: `${age}It’s waiting on you, so it opens where it lives and you answer there.`, where }
          : { icon: 'external', title: `Open “${x.title}”`, why: `${age}${rowWhy(x.detail, x.headline, now)}`, where };
      }
      const why = !x.live
        ? (x.harness === 'claude' || x.harness === 'codex') && x.surface === 'cli'
          ? 'It isn’t running: it resumes in a new terminal with this prompt.'
          : 'It isn’t running: it opens with this prompt ready to send.'
        : x.status === 'working'
          ? 'It’s working, so this waits for its turn to end.'
          : x.harness === 'claude' && x.surface === 'app'
            ? 'It gets this as a message from another session, in the Claude app.'
            : 'It gets this as its next message.';
      return { icon: 'send', title: `Send to “${x.title}”`, why: `${age}${why}`, where };
    }
    if (sg.kind === 'todo') {
      return { icon: 'list-check', title: `Save as a todo: “${todoTitle(text)}”`, why: `In ${sg.lane.project}, linked to ${sg.lane.callsign}. Start it when you’re ready.`, where: sg.lane.callsign };
    }
    const tree = claudeTerminal && sg.newWorktree;
    const who = account ? `${account}${onModel}` : 'a session';
    const why =
      placeNow === 'claude-app'
        ? `The Claude app opens a new session in ${sg.lane.callsign} with this prompt filled in.`
        : placeNow === 'codex-app'
          ? `Codex opens a new thread in ${sg.lane.callsign} with this prompt filled in.`
          : tree
            ? `Claude Code makes a new worktree of ${sg.lane.repo} and starts ${who} there${placeNow === 'background' ? ' in the background' : ''}.`
            : `Starts ${who} in ${sg.lane.callsign}${placeNow === 'background' ? ' in the background' : ''}.`;
    // The warning leads: the row's line is cut short when it's long.
    const warned = duplicate ? `“${duplicate.session.title}” may already be on this. ${why}` : why;
    return { icon: 'plus', title: tree ? `Start in a new worktree of ${sg.lane.repo}` : `Start in ${sg.lane.origin === 'main' ? `${sg.lane.callsign} (main)` : sg.lane.callsign}`, why: warned, where: `${sg.lane.project} · ${sg.lane.repo}` };
  };

  /* ---- Render ---- */

  const sections: { group: RowGroup; rows: Row[] }[] = [];
  for (const r of rows) {
    const last = sections[sections.length - 1];
    if (last && last.group === r.group) last.rows.push(r);
    else sections.push({ group: r.group, rows: [r] });
  }
  const nothingWaits = !query && !!attention && attention.counts.needs + attention.counts.unread === 0;
  /** A group's heading count: all there are of it, when the list shows only some. */
  const groupCount = (g: RowGroup, shown: number): number => {
    if (g === 'ready') return readyList?.count ?? shown;
    if (g === 'recent') return recent?.count ?? shown;
    if (g === 'needs' || g === 'unread' || g === 'working') return attention?.counts[g] ?? shown;
    return shown;
  };
  const agentLabel = autoFor ? 'Auto' : (account ?? 'No account');
  const agentDetail = [autoFor ? account : null, modelShown].filter(Boolean).join(' · ');
  const shortcut = shortcutLabel(board?.quickShortcut);

  /** A reply went through, or what it waited on was answered: the panel is done. The window that
   *  opened it hears what happened. */
  const replied = (message: string | null) => {
    setReplyKey(null);
    setActiveKey(null);
    void hideQuick();
    if (message) void tellOpener(opener ?? 'main', { phase: 'end', id: Date.now(), key: null, message });
  };

  return (
    <div
      className="ca-quick-root"
      onMouseDown={(e) => {
        // The shadow around the card is part of the window; a click there is a click outside.
        if (e.target === e.currentTarget) void hideQuick();
      }}
    >
      {replying && (
        <div ref={card} className="ca-quick is-reply" role="dialog" aria-label={`Answer ${replying.session.title}`} style={{ maxHeight: Math.min(REPLY_HEIGHT, room ? room - 2 * MARGIN : REPLY_HEIGHT) }}>
          <ReplyView
            lane={replying.lane}
            session={replying.session}
            terminal={board?.apps.terminal ?? 'Terminal'}
            now={now}
            autoFocus
            keys
            onBack={leaveReply}
            onOpen={() => openSession(replying.lane, replying.session)}
            onReveal={() => revealSession(replying.lane, replying.session)}
            onSeen={() => dismissReply(markSeen([replying.session.key]))}
            onArchive={() => dismissReply(archiveSession(replying.session.key, true))}
            onSent={(sent) => replied(sent.message)}
            onAnswered={(said) => replied(said)}
          />
        </div>
      )}
      {!replying && (
      <div ref={card} className="ca-quick" role="dialog" aria-label="Codealong" style={room ? { maxHeight: room - 2 * MARGIN } : undefined}>
        <div ref={inputBox} className="ca-quick-head">
        <div className="ca-quick-input">
          <div className="ca-quick-pick">
            <button type="button" data-picker-trigger className="ca-quick-project" title="Project (⌘P)" aria-haspopup="dialog" aria-expanded={picker === 'project'} onClick={() => togglePicker('project')}>
              <Mark size={16} status={logoStatus} />
              <span>{current}</span>
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'project' && <Picker sections={projectSections} value={current} placeholder="Project" onPick={pickProject} onClose={() => { setPicker(null); refocus(); }} />}
          </div>
          <textarea
            ref={input}
            value={text}
            rows={1}
            spellCheck
            aria-label="Find a session, or describe new work"
            placeholder={draft ? 'Find a session, or describe new work… ↑ brings back your draft' : history.length ? 'Find a session, or describe new work… ↑ for earlier prompts' : 'Find a session, or describe new work…'}
            onChange={(e) => {
              setText(e.target.value);
              setRecall(null);
              setActiveKey(null);
            }}
            onKeyDown={onInputKey}
            onPaste={(e) => {
              const files = filesToAttach(e);
              if (files.length === 0) return;
              e.preventDefault();
              keepFiles(files)
                .then(addAttachments)
                .catch((err: unknown) => setError(plain(err)));
            }}
          />
          {shortcut && !text && (
            <span className="ca-quick-hotkey" title="Opens this panel from any app. Change it in Settings.">
              {shortcut}
            </span>
          )}
        </div>
        {attached.length > 0 && (
          <div className="ca-quick-attached">
            <AttachmentStrip items={attached} onRemove={(a) => setText(withoutAttachment(text, a.path))} />
          </div>
        )}
        </div>
        <div ref={list} className="ca-quick-list" role="listbox" aria-label="What to do next">
          <div ref={scroller} className="ca-quick-scroll">
            {!board ? (
              <p className="ca-quick-note">{error ? '' : 'Reading sessions…'}</p>
            ) : (
              <>
                {nothingWaits && (
                  <p className="ca-quick-note">
                    Nothing needs you{attention.counts.working > 0 ? ` · ${attention.counts.working} working` : ''}.
                  </p>
                )}
                {routing && (
                  <p className="ca-quick-note is-routing">
                    <Icon name="sparkles" size={12} />
                    Working out where this goes…
                  </p>
                )}
                {routeProblem && <p className="ca-quick-note">Couldn’t suggest where this goes: {routeProblem}</p>}
                {query && rows.length === 0 && <p className="ca-quick-note">No sessions, todos or PRs match. Start a session with this prompt below.</p>}
                {query && textFound?.building && <p className="ca-quick-note">Still reading what recent sessions said, so more may match in a moment.</p>}
                {sections.map((s) => (
                  <div key={s.group} className="ca-quick-group" role="group" aria-label={GROUP_WORD[s.group]}>
                    <div className={`ca-quick-glabel is-${s.group}`}>
                      {GROUP_WORD[s.group]}
                      <span>{groupCount(s.group, s.rows.length)}</span>
                    </div>
                    {s.rows.map((r) => {
                      const n = rows.indexOf(r) + 1;
                      // What every row does with the pointer: hovering selects, a click does it, a
                      // right-click has the rest.
                      const handlers = {
                        onMouseMove: () => active !== r.key && setActiveKey(r.key),
                        onMouseDown: (e: React.MouseEvent) => e.preventDefault(),
                        onClick: (e: React.MouseEvent) => activate(r.key, e.altKey),
                        onContextMenu: (e: React.MouseEvent) => {
                          e.preventDefault();
                          rowMenu(r);
                        },
                      };
                      if (r.kind === 'suggest') {
                        const said = describe(r.s);
                        const pct = Math.round(r.s.p * 100);
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={['ca-quick-row', 'is-suggest', active === r.key && 'is-active'].filter(Boolean).join(' ')}
                            title={`${said.title}\n${said.why}\n${pct}% likely, from ${route?.model ?? 'Jev'} in ${route?.ms ?? 0} ms · ⏎ does it${r.s.kind === 'send' || r.s.kind === 'open' ? ' · ⌥⏎ shows the session in Codealong' : ''}`}
                            {...handlers}
                          >
                            <span className="ca-quick-plus is-suggest">
                              <Icon name={said.icon} size={13} stroke={2} />
                            </span>
                            <span className="ca-quick-row-main">
                              <span className="ca-quick-row-title">{said.title}</span>
                              <span className="ca-quick-row-why">{said.why}</span>
                            </span>
                            <span className="ca-quick-row-where">{said.where}</span>
                            <span className={`ca-quick-sure is-${sureness(r.s.p)}`}>{pct}%</span>
                            {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                          </div>
                        );
                      }
                      if (r.kind === 'todo') {
                        const t = r.item.todo;
                        const opens = todoSession(r) ? 'opens its session' : 'gets a session ready to start for it';
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={['ca-quick-row', 'is-todo', active === r.key && 'is-active'].filter(Boolean).join(' ')}
                            title={`${t.title || 'Untitled'}\n⏎ ${opens} · ⌥⏎ shows it in Codealong`}
                            {...handlers}
                          >
                            <TodoGlyph status={t.status} waiting={r.waiting} />
                            <span className="ca-quick-row-main">
                              <span className="ca-quick-row-title">
                                <Marked text={t.title || 'Untitled'} marks={r.marks} />
                              </span>
                              <span className="ca-quick-row-why">{todoWhy(r)}</span>
                            </span>
                            <span className="ca-quick-row-where">{r.item.lane?.callsign ?? (t.project || 'No worktree')}</span>
                            <span className="ca-quick-row-age">{ago(r.item.lastActivity, now)}</span>
                            {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                          </div>
                        );
                      }
                      if (r.kind === 'pr') {
                        const pr = r.pr;
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={['ca-quick-row', 'is-pr', active === r.key && 'is-active'].filter(Boolean).join(' ')}
                            title={`${pr.title}\n${pr.repoSlug}#${pr.number} · ${pr.head}\n⏎ opens it on GitHub · ⌥⏎ shows it in Codealong`}
                            {...handlers}
                          >
                            <PrGlyph pr={pr} />
                            <span className="ca-quick-row-main">
                              <span className="ca-quick-row-title">
                                <Marked text={pr.title} marks={r.marks} />
                              </span>
                              <span className="ca-quick-row-why">{prWhy(pr)}</span>
                            </span>
                            <span className="ca-quick-row-where">{pr.repo}</span>
                            <span className="ca-quick-row-age">{ago(pr.updatedAt, now)}</span>
                            {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                          </div>
                        );
                      }
                      const s2 = r.session;
                      return (
                        <div
                          key={r.key}
                          role="option"
                          aria-selected={active === r.key}
                          className={['ca-quick-row', active === r.key && 'is-active', `is-${s2.status}`].filter(Boolean).join(' ')}
                          title={`${s2.title}\n${s2.account} · ${r.lane.callsign}\n⏎ opens it where it lives · ⇥ answers it here · ⌥⏎ shows it in Codealong`}
                          {...handlers}
                        >
                          <StatusGlyph status={s2.status} />
                          <span className="ca-quick-row-main">
                            <span className="ca-quick-row-title">
                              <Marked text={s2.title} marks={r.marks} />
                            </span>
                            {r.said ? (
                              <span className="ca-quick-row-why is-said">
                                {r.said.you ? 'You: ' : ''}
                                <Marked text={r.said.text} marks={r.said.marks} />
                              </span>
                            ) : (
                              <span className="ca-quick-row-why">{rowWhy(s2.detail, s2.headline, now)}</span>
                            )}
                          </span>
                          <span className="ca-quick-row-where">
                            {s2.account} · {r.lane.callsign}
                          </span>
                          <span className="ca-quick-row-age">{ago(s2.statusSince ?? s2.lastActivity, now)}</span>
                          {active === r.key && <span className="ca-quick-row-tab">⇥ Reply</span>}
                          {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                        </div>
                      );
                    })}
                  </div>
                ))}
              </>
            )}
          </div>
          <div
            role="option"
            aria-selected={active === COMPOSE}
            className={['ca-quick-row', 'ca-quick-compose', active === COMPOSE && 'is-active', !ready && 'is-disabled'].filter(Boolean).join(' ')}
            onMouseMove={() => active !== COMPOSE && setActiveKey(COMPOSE)}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => start()}
          >
            <span className="ca-quick-plus">
              <Icon name="plus" size={14} stroke={2} />
            </span>
            <span className="ca-quick-row-main">
              <span className="ca-quick-row-title">{forTodos.length > 1 ? `Start one session for these ${forTodos.length} todos` : todo ? 'Start a session for this todo' : prompted ? 'Start a new session with this prompt' : 'Start a new session'}</span>
              <span className="ca-quick-row-why">
                {happens}
                {alreadyOn}
                {attached.length > 0 && ` ${attached.length === 1 ? 'The attachment goes' : 'Attachments go'} as ${attached.length === 1 ? 'a file path' : 'file paths'} the agent opens.`}
                {newToCodex && ' Codex adds this folder as a project if it isn’t one yet.'}
              </span>
            </span>
            <Kbd keys={['⌘', '⏎']} />
          </div>
        </div>
        {todo && (
          <div className="ca-quick-todo">
            {forTodos.length > 1 ? <Icon name="list-check" size={14} /> : <TodoGlyph status={todo.status} />}
            <span className="ca-quick-todo-title" title={forTodos.length > 1 ? forTodos.map((t, i) => `${i + 1}. ${t.title || 'Untitled'}`).join('\n') : undefined}>
              {forTodos.length > 1 ? `For ${forTodos.length} todos: ${forTodos.map((t) => `“${t.title || 'Untitled'}”`).join(', ')}` : `For “${todo.title || 'Untitled'}”`}
            </span>
            {waitsOn.length > 0 && (
              <span className="ca-quick-todo-wait" title={blocked.flatMap((b) => b.on.map((w) => `${forTodos.length > 1 ? `“${b.t.title || 'Untitled'}” waits` : 'Waits'} on “${w.title || 'Untitled'}”`)).join('\n')}>
                <Icon name="lock" size={12} />
                {forTodos.length > 1 && (blocked.length === 1 ? 'one ' : `${blocked.length} `)}
                {forTodos.length > 1 && blocked.length > 1 ? 'wait' : 'waits'} on {waitsOn.length === 1 ? 'a todo that isn’t done' : `${waitsOn.length} todos that aren’t done`}
              </span>
            )}
            {marked.length > 0 && (
              <span className="ca-quick-todo-wait" title={`Starting a session on ${forTodos.length > 1 ? 'them' : 'it'} ends the wait.`}>
                <Icon name="hourglass" size={12} />
                {forTodos.length > 1 ? `${marked.length === 1 ? 'one' : marked.length} marked as waiting` : todo.waiting?.note ? `waiting for ${todo.waiting.note}` : 'marked as waiting'}
              </span>
            )}
            <button type="button" className="ca-linkbtn" title={forTodos.length > 1 ? "Start a session that isn't linked to these todos" : "Start a session that isn't linked to this todo"} onClick={() => setTodoIds([])}>
              Don’t link
            </button>
          </div>
        )}
        {error && (
          <p className="ca-quick-error" role="alert">
            <Icon name="alert" size={14} />
            <span>{error}</span>
          </p>
        )}
        <div className="ca-quick-bar">
          <div className="ca-quick-pick">
            <button type="button" data-picker-trigger className="ca-quick-chip" title={lane ? `${tildePath(lane.path)}\nWorktree (⌘T)` : 'Worktree (⌘T)'} disabled={lanes.length === 0} aria-haspopup="dialog" aria-expanded={picker === 'worktree'} onClick={() => togglePicker('worktree')}>
              <Icon name="branch" size={14} />
              <span>{lane ? (lane.origin === 'main' ? `${lane.callsign} (main)` : lane.callsign) : 'No worktree'}</span>
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'worktree' && <Picker sections={worktreeSections} value={lane?.id ?? null} placeholder="Worktree, branch or repo" onPick={(id) => { setLaneId(id); setPicker(null); refocus(); }} onClose={() => { setPicker(null); refocus(); }} />}
          </div>
          <label className={['ca-check', 'ca-quick-newtree', !claudeTerminal && 'is-disabled'].filter(Boolean).join(' ')} title={claudeTerminal ? 'Claude Code makes the worktree: its folder, branch and base.' : placeNow === 'terminal' ? 'Only Claude Code can start in a new worktree from here.' : 'The app has its own worktree choice; pick it there before you send.'}>
            <input type="checkbox" checked={inNewWorktree} disabled={!claudeTerminal} onChange={(e) => setNewWorktree(e.target.checked)} />
            <span className="ca-check-box" aria-hidden="true">
              <Icon name="check" size={11} stroke={2.5} />
            </span>
            New worktree
          </label>
          <div className="ca-quick-pick ca-quick-agent">
            <button type="button" data-picker-trigger className="ca-quick-chip" title="Account and model (⌘L)" disabled={accounts.length === 0} aria-haspopup="dialog" aria-expanded={picker === 'agent'} onClick={() => togglePicker('agent')}>
              <Icon name={autoFor ? 'sparkles' : 'user'} size={14} />
              <span>{accounts.length === 0 ? 'No accounts' : agentLabel}</span>
              {agentDetail && <span className="ca-quick-chip-detail">{agentDetail}</span>}
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'agent' && <Picker className="is-wide" sections={agentSections} value={agentNow} placeholder="Account or model" onPick={pickAgent} onClose={() => { setPicker(null); refocus(); }} />}
          </div>
          {places.length > 1 ? (
            <div className="ca-segctl ca-quick-places" role="radiogroup" aria-label="Opens in">
              {places.map((p) => (
                <button key={p.place} type="button" role="radio" aria-checked={placeNow === p.place} className={placeNow === p.place ? 'is-on' : undefined} onClick={() => pickPlace(p.place)}>
                  <Icon name={placeIcon(p.place)} size={13} />
                  {p.label}
                </button>
              ))}
            </div>
          ) : places.length === 1 ? (
            <span className="ca-quick-place">
              <Icon name={placeIcon(places[0].place)} size={13} />
              {places[0].label}
            </span>
          ) : null}
        </div>
      </div>
      )}
    </div>
  );
}

/** Whether a place starts the account's command, which takes a model and --worktree. */
function onCommandLine(place: StartPlace): boolean {
  return place === 'terminal' || place === 'background';
}

function placeIcon(place: StartPlace): string {
  return place === 'terminal' ? 'terminal' : place === 'background' ? 'stack' : 'app';
}

function ProjectCounts({ needs, unread }: { needs: number; unread: number }) {
  return (
    <span className="ca-quick-counts">
      {needs > 0 && (
        <span className="ca-projswitch-action is-needs" title={`${needs} need you`}>
          <StatusGlyph status="needs" size={10} title="" />
          {needs}
        </span>
      )}
      {unread > 0 && (
        <span className="ca-projswitch-action is-unread" title={`${unread} finished`}>
          <StatusGlyph status="unread" size={10} title="" />
          {unread}
        </span>
      )}
    </span>
  );
}

function AccountHead({ name, detail, meters, note, now }: { name: string; detail: string; meters: Meter[] | null; note: string; now: number }) {
  return (
    <span className="ca-quick-acct">
      <span className="ca-quick-acct-name">{name}</span>
      <span className="ca-quick-acct-detail">{detail}</span>
      <span className="ca-quick-acct-meters">{meters && meters.length > 0 ? meters.map((m) => <UsageMeter key={m.key} meter={m} now={now} />) : <span className="ca-pick-note">{note}</span>}</span>
    </span>
  );
}
