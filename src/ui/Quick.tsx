// The quick panel's page (quick.rs has its window): one input, and below it what you can do
// next. With nothing typed it lists the sessions that need you, then the ones whose turn ended,
// then a few todos ready to start, then the other sessions started in the last day; typed words find sessions, todos and PRs; the last row starts a
// new session with what you typed as its prompt, in the worktree, on the agent and in the app
// chosen at the bottom.
//
// ⏎ does the selected row and ⌘⏎ always starts. ⌥⏎ shows the session in Codealong, ⌘1 to ⌘9
// open a row, ⌘P picks the project and ⌥⌘1 to ⌥⌘9 switch to one (⌥⌘0 to all of them), ⌘D picks
// the Mac (with others linked), ⌘T the worktree and ⌘L the agent, whose menu has the effort
// under its models (← → there), ↑ in the empty input brings back the last draft and earlier
// prompts, and Escape closes. (⌘M is the Window menu's Minimize, which may take the key before
// the page sees it.)
//
// ⇥ on a session answers it here: the panel shows its conversation and a box that sends it a
// message where it runs, or what it waits on you to allow (ReplyView.tsx). ⏎ sends, and Escape
// goes back to the list, as marking it seen or archiving it from its header does. Answering one
// from the empty panel's list works through the list: once it's sent, allowed, marked seen,
// snoozed or archived, the next that waits on you takes the view (the longest wait first), ⌘]
// skips to it, and after the last one the panel closes.

import { reuse } from '../reuse';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import {
  deviceStartOptions,
  fitQuick,
  getBoard,
  getDevices,
  hideQuick,
  inPreview,
  onBoard,
  onDevices,
  onQuickHidden,
  openFromBar,
  openUrl,
  quickReady,
  routeQuick,
  searchText,
  sessionCancel,
  showMain,
  showQuick,
  startFromQuick,
  startOptions,
  tellOpener,
  todoOp,
  type QuickOpen,
} from '../api';
import { archiveAhead, boardAhead, boardView, observe, renameAhead, seenAhead, sendAhead, useAhead } from '../ahead';
import { filesToAttach, keepFiles, useFileDrop } from '../attachInput';
import { deviceLabel, shortOf } from '../devices';
import { useKeyPerf, usePerf } from '../perf';
import { below, showContextMenu } from '../contextMenu';
import { ago, agoPhrase, resetsPhrase, tildePath } from '../format';
import { HARNESS_WORD, renames, usageNote } from '../harness';
import { useLogo } from '../logo';
import { prPills } from '../prStatus';
import { stoppable } from '../lanes';
import { snoozeMenu } from '../snooze';
import { ALL_PROJECTS, attentionRows, findPrs, findSessions, findTodos, FOUND_SESSIONS, inProject, laneBranch, laneGroups, MAX_ROWS, projectCounts, queryWords, queueAfter, queueRows, readyTodos, recallList, recentRows, shortcutLabel, stillSaid, TEXT_SEARCH_MIN, type Handled, type PrRow, type RowGroup, type SessionRow, type TodoRow, withPrompt as historyWith } from '../quick';
import {
  accountFor,
  agentId,
  agentOptions,
  autoModel,
  autoPick,
  initialChoice,
  isAuto,
  preferredNames,
  modelFamily,
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
import type { Attachment, Board, DevicesView, Lane, Meter, PrItem, Route, Session, StartOptions, StartPlace, StartSpec, TextFound } from '../types';
import { AttachmentStrip } from './DescriptionEditor';
import { sessionWhy } from './PanelSession';
import { Picker, type PickerOption, type PickerSection } from './Picker';
import { ReplyView } from './ReplyView';
import { Icon, Kbd, Mark, STATUS_WORD, StatusGlyph } from './primitives';
import { projectForDigit, projectKey } from './ProjectSwitcher';
import { PrGlyph } from './PrsView';
import { applyTheme } from './ThemeSwitch';
import { TodoGlyph } from './TodoBits';
import { UsageMeter } from './UsageBar';

declare global {
  interface Window {
    /** Opens the panel for what Codealong asks, with the board, and returns its height once it's
     *  laid out (quick.rs calls it). */
    __quickOpen?: (context: QuickOpen | null, board: Board | null) => number;
    /** An open that came before the page was up, for the page to do once it is. */
    __quickPending?: [QuickOpen | null, Board | null];
  }
}

/** Room around the card for its shadow. The window (quick.rs) is the card and this on each side. */
const MARGIN = 20;
const PROJECT_KEY = 'codealong.quick.project';
const HISTORY_KEY = 'codealong.quick.history';
/** The input grows to this many lines, then scrolls. */
const INPUT_LINES = 8;
const COMPOSE = 'compose';
/** How long a read of where accounts can start is used without waiting for a new one. */
const OPTIONS_FOR_MS = 60_000;
/** How long after another Mac didn't say where its accounts can start it's asked again. */
const RETRY_MS = 5_000;
/** The device menu's id for this Mac; the others go by their short ids, which are hex. */
const HERE = 'here';
/** The tallest the card gets while it answers a session; the conversation scrolls inside. */
const REPLY_HEIGHT = 640;

type PickerKind = 'project' | 'device' | 'worktree' | 'agent';

const GROUP_WORD: Record<RowGroup, string> = { suggested: 'Suggested', needs: 'Needs you', unread: 'Your turn', working: 'Working', found: 'Sessions', todos: 'Todos', prs: 'PRs', ready: 'Ready todos', recent: 'Last 24 hours' };

/** A suggestion from Jev, as a row. */
interface SuggestRow {
  kind: 'suggest';
  key: string;
  group: 'suggested';
  s: Suggestion;
}

/** Under a search's first few sessions, the rest of them, a click away. */
interface MoreRow {
  kind: 'more';
  key: string;
  group: 'found';
  count: number;
}

type Row = SessionRow | TodoRow | PrRow | SuggestRow | MoreRow;

/** The most sessions a search lists once you ask for all of them. */
const FOUND_ALL = 40;

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
  const [engineBoard, setBoard] = useState<Board | null>(null);
  // Changes made in any window that the engine hasn't shown yet (ahead.ts), on top of the board.
  const ahead = useAhead();
  const board = useMemo(() => (engineBoard ? boardAhead(engineBoard, ahead) : null), [engineBoard, ahead]);
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
  // `--effort` for the agent chosen; null is the account's own.
  const [effort, setEffort] = useState<string | null>(null);
  const [place, setPlace] = useState<StartPlace | null>(null);
  // The Mac a new session starts on, by its short id; null is this one.
  const [device, setDevice] = useState<string | null>(null);
  const [devices, setDevices] = useState<DevicesView | null>(null);
  // Where accounts can start on your other Macs, asked of each once it's chosen.
  const [remoteOptions, setRemoteOptions] = useState<Record<string, { at: number; options: StartOptions | null; error: string | null }>>({});
  const [newWorktree, setNewWorktree] = useState(false);
  // The todos a new session is for: one, or several started together on one prompt.
  const [todoIds, setTodoIds] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [picker, setPicker] = useState<PickerKind | null>(null);
  // The session the panel answers in place of the list (⇥ on its row).
  const [replyKey, setReplyKey] = useState<string | null>(null);
  // The session shown with its name ready to change (Rename in a row's menu).
  const [renameKey, setRenameKey] = useState<string | null>(null);
  // Answered from the empty list: the view goes on to the next session that waits on you as each
  // one's done, and says how the last one went.
  const [queued, setQueued] = useState(false);
  const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
  const handled = useRef<Handled>(new Map());
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
  // While an open lays the panel out: 'script' when Codealong's script opens it (its height goes
  // back as the script's result), 'page' when the page opened it itself (it sends its height).
  const opening = useRef<'script' | 'page' | null>(null);
  // The height last reported for this open, by the open or by a fit.
  const reported = useRef(0);
  // When a board push arrived, for the perf log.
  const arrived = useRef<number | null>(null);

  // Opening. Codealong opens the panel with a script that calls `window.__quickOpen` with what
  // it opens for and the board, and shows the panel at the height it returns. WebKit runs that
  // script at the page's full priority, and whatever a hidden page does after it at background
  // priority, which a busy Mac starves for seconds (quick.rs): so the whole open, laid out and
  // measured, happens before the script returns. Where accounts can start comes from a copy
  // read earlier, and a stale one is read again after.
  useEffect(() => {
    let reading = false;
    const readOptions = () => {
      if (reading) return;
      reading = true;
      startOptions()
        .then((o) => {
          cachedOptions.current = { at: Date.now(), options: o };
          setOptions(o);
        })
        // A failed read leaves the copy there is.
        .catch(() => {})
        .finally(() => {
          reading = false;
        });
    };
    const apply = (ctx: QuickOpen, b: Board | null) => {
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
        setEffort(null);
        setPlace(null);
        setDevice(ctx.laneId ? shortOf(ctx.laneId) : null);
        setNewWorktree(false);
        setActiveKey(null);
        setReplyKey(ctx.reply ?? null);
        setQueued(false);
        setNotice(null);
      }
      if (b) {
        setBoard(b);
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
      }
      setShown(true);
      setOpens((n) => n + 1);
    };
    const refresh = (b: Board | null) => {
      if (!cachedOptions.current || Date.now() - cachedOptions.current.at >= OPTIONS_FOR_MS) readOptions();
      // Your other Macs, for starting there, when there are any; while the panel is up it
      // follows them.
      if (b?.devices?.enabled)
        getDevices()
          .then((d) => d && setDevices(d))
          .catch(() => {});
    };
    const openNow = (by: 'script' | 'page', ctx: QuickOpen, b: Board | null) => {
      opening.current = by;
      try {
        flushSync(() => apply(ctx, b));
      } finally {
        opening.current = null;
      }
      refresh(b);
    };
    // Opened by Codealong's script: laid out and measured before it returns.
    window.__quickOpen = (ctx, b) => {
      openNow('script', ctx ?? {}, b);
      return reported.current;
    };
    // Opened some other way (the preview, or a script that came before the page was up): the
    // page reads the board itself if it has to, and sends its height once it's laid out.
    const openLater = (ctx: QuickOpen, b: Board | null) => {
      (b ? Promise.resolve(b) : getBoard())
        .then((fresh) => openNow('page', ctx, fresh))
        .catch((e: unknown) => {
          setError(plain(e));
          openNow('page', ctx, null);
        });
    };
    readOptions();
    // Within the script that says so, like an open: the board pushes stop at once, not whenever
    // the hidden page next gets to run.
    const offHidden = onQuickHidden(() =>
      flushSync(() => {
        setShown(false);
        setPicker(null);
      }),
    );
    const early = window.__quickPending;
    delete window.__quickPending;
    if (early) openLater(early[0] ?? {}, early[1]);
    // A browser preview has no engine to open it.
    else if (inPreview) openLater({}, null);
    return () => {
      delete window.__quickOpen;
      offHidden();
    };
  }, []);

  useEffect(
    () =>
      shown
        ? onBoard((b) => {
            arrived.current = performance.now();
            setBoard((prev) => (prev ? reuse(prev, b) : b));
          })
        : undefined,
    [shown],
  );
  useEffect(() => (shown ? onDevices(setDevices) : undefined), [shown]);
  usePerf(engineBoard, arrived);
  // The panel follows the board only while it's up; so does what it knows of its changes.
  useEffect(() => {
    if (shown && engineBoard) observe(boardView(engineBoard.generatedAt, [engineBoard.lanes]));
  }, [shown, engineBoard]);
  // Keys typed anywhere in the panel, its reply box included.
  const keyed = useKeyPerf(board?.perfLog ?? false);
  useEffect(() => {
    window.addEventListener('keydown', keyed, true);
    return () => window.removeEventListener('keydown', keyed, true);
  }, [keyed]);

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
  // The board lists no projects until it's read, so the one picked holds until then.
  const current = project === ALL_PROJECTS || projects.some((p) => p.name === project) || !board || board.loading ? project : ALL_PROJECTS;
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

  // Which Mac: this one, or another linked Mac, which starts the session with its own accounts
  // in its own worktrees. Until your devices are read, one a window asked for is taken as it is.
  const macs = useMemo(() => (devices?.enabled ? devices.devices : []), [devices]);
  const deviceNow = device && (!devices || macs.some((d) => d.short === device)) ? device : null;
  const deviceInfo = macs.find((d) => d.short === deviceNow) ?? null;
  const remote = deviceNow ? remoteOptions[deviceNow] : undefined;
  // Where its accounts can start is asked of it, and asked again once it's a minute old.
  useEffect(() => {
    if (!shown || !deviceNow || (remote && Date.now() - remote.at < (remote.error ? RETRY_MS : OPTIONS_FOR_MS))) return;
    const d = deviceNow;
    deviceStartOptions(d)
      .then((o) => setRemoteOptions((r) => ({ ...r, [d]: { at: Date.now(), options: o, error: null } })))
      .catch((e: unknown) => setRemoteOptions((r) => ({ ...r, [d]: { at: Date.now(), options: r[d]?.options ?? null, error: plain(e) } })));
  }, [shown, deviceNow, remote]);
  const startOptionsNow = deviceNow ? (remote?.options ?? null) : options;

  // Where a session can start: the project's worktrees on that Mac, and the one a window asked for.
  const lanes = useMemo(() => {
    const all = deviceNow ? (devices?.lanes ?? []).filter((l) => shortOf(l.id) === deviceNow) : (board?.lanes ?? []);
    const mine = all.filter((l) => inProject(l, current));
    const extra = laneId && !mine.some((l) => l.id === laneId) ? all.filter((l) => l.id === laneId) : [];
    return [...mine, ...extra];
  }, [board?.lanes, devices?.lanes, deviceNow, current, laneId]);
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
  // What it works through, and what's left after the one showing. Snoozed ones wait their time.
  const queue = useMemo(() => (board && queued ? queueRows(board, current).filter((r) => !r.session.snoozedUntil) : []), [board, queued, current]);
  const queueLeft = replying && queued ? queueAfter(queue, replying.session.key, handled.current) : [];
  // Read when a send fails, which can be long after the view went on to another session.
  const replyKeyNow = useRef(replyKey);
  replyKeyNow.current = replyKey;
  const shownNow = useRef(shown);
  shownNow.current = shown;
  // How the last one went shows for a moment, then what's left does; why one failed stays.
  useEffect(() => {
    if (!notice || notice.error) return;
    const t = window.setTimeout(() => setNotice(null), 4000);
    return () => window.clearTimeout(t);
  }, [notice]);

  /* ---- Who ---- */

  // Another Mac's usage comes with its feed.
  const usage = useMemo(() => (deviceNow ? (deviceInfo?.usage ?? []) : (board?.usage ?? [])), [deviceNow, deviceInfo?.usage, board?.usage]);
  // Stars come with the board, which is current; where accounts can start is read less often.
  const accounts = useMemo(() => (startOptionsNow?.accounts ?? []).map((a) => ({ ...a, preferred: usage.find((u) => u.account === a.name)?.preferred ?? a.preferred })), [startOptionsNow, usage]);
  const models = useMemo(() => startOptionsNow?.claudeModels ?? [], [startOptionsNow]);
  const efforts = useMemo(() => startOptionsNow?.claudeEfforts ?? [], [startOptionsNow]);
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

  // A model or effort is asked for on the command line (or of Codealong's adapter), so choosing
  // one leaves only the places that take it; the apps pick their own.
  const allPlaces = placesFor(choice, accounts, pick);
  const tuned = !!chosenModel || (!!effort && harness === 'claude' && efforts.length > 0);
  const places = tuned ? allPlaces.filter((p) => placeTakesModel(p.place)) : allPlaces;
  const placeNow = [place, choice ? (settledPlace(choice, accounts, pick) ?? remembered().places?.[choice]) : undefined].map((p) => places.find((x) => x.place === p)?.place).find(Boolean) ?? places[0]?.place ?? null;
  const claudeTerminal = !!placeNow && onCommandLine(placeNow) && harness === 'claude';
  // Codealong hands the model to Claude Code's adapter, which takes the same names.
  const claudeModel = !!placeNow && placeTakesModel(placeNow) && harness === 'claude';
  const modelNow = claudeModel ? (autoFor ? (autoModelPick?.model ?? null) : chosenModel) : null;
  const inNewWorktree = claudeTerminal && newWorktree;
  const defaultModel = accountInfo?.defaultModel ?? null;
  const modelShown = !claudeModel
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
  // Claude Code starts on the model asked for, else the account's own.
  const startsOn = claudeModel ? (modelNow ?? defaultModel) : null;
  // The effort goes wherever the model does, except to Haiku, which takes none. A Mac whose
  // Codealong doesn't know effort yet lists no levels.
  const effortTaken = claudeModel && efforts.length > 0 && modelFamily(startsOn) !== 'haiku';
  const effortNow = effortTaken ? effort : null;
  const effortWord = efforts.find((e) => e.id === effortNow)?.label ?? null;

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
  // Until it answers, the last search's hits whose lines still have every word stay in the list.
  const hits = useMemo(() => (textFound ? textFound.hits : textFor && textSearch ? stillSaid(textSearch.found.hits, queryWords(textFor)) : []), [textFound, textFor, textSearch]);
  const said = useMemo(() => new Map(hits.map((h) => [h.key, h] as const)), [hits]);
  const foundSessions = useMemo(() => (board && query ? findSessions(board, current, query, FOUND_ALL, said) : []), [board, current, query, said]);
  // A search lists its first few sessions, so todos and PRs fit below; a row under them lists
  // the rest, until what's typed changes.
  const [allFor, setAllFor] = useState<string | null>(null);
  const showAll = allFor === query;
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
  // Jev knows this Mac's worktrees, so it suggests starting only while a session would start here.
  const suggested = useMemo<SuggestRow[]>(
    () => (board && route ? suggestions(route, board).filter((sg) => !deviceNow || sg.kind !== 'start').map((sg) => ({ kind: 'suggest', key: sg.key, group: 'suggested', s: sg })) : []),
    [board, route, deviceNow],
  );

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
    if (opening.current !== 'script') void quickReady(h);
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

  const ranked = rankedBy(foundSessions, route);
  const moreRow: MoreRow[] = !showAll && ranked.length > FOUND_SESSIONS ? [{ kind: 'more', key: 'more:found', group: 'found', count: ranked.length - FOUND_SESSIONS }] : [];
  const rows: Row[] = query ? [...suggested, ...(showAll ? ranked : ranked.slice(0, FOUND_SESSIONS)), ...moreRow, ...foundTodos, ...foundPrs] : [...(attention?.rows ?? []), ...(readyList?.rows ?? []), ...(recent?.rows ?? [])];
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

  /** Brings the panel back as you left it, with why what you asked failed. What's put back is
   *  drawn first: the script that reopens the panel lays out the open's own changes at once, and
   *  changes made outside the page's events would come a moment after (`opening`, above). */
  const reopen = (restore: () => void) => {
    flushSync(restore);
    void showQuick({ keep: true });
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
    openFromBar(lane.id, session.openTarget).catch((e: unknown) =>
      reopen(() => {
        setText(kept);
        setError(plain(e));
      }),
    );
  };

  const revealSession = (lane: Lane, session: Session) => {
    void hideQuick();
    void showMain({ kind: 'session', laneId: lane.id, key: session.key });
  };

  /** Answers a session here: its conversation, and a box for the reply. What's typed in the input
   *  stays for when you go back. */
  const replyTo = (session: Session) => {
    const fromList = !!attention?.rows.some((r) => r.key === session.key && r.group !== 'working');
    if (fromList) handled.current = new Map();
    setQueued(fromList);
    setNotice(null);
    setReplyKey(session.key);
    setRenameKey(null);
    setPicker(null);
    setError(null);
  };

  /** Shows a session with its name ready to change; done, it's back to the list. */
  const renameIn = (session: Session) => {
    setQueued(false);
    setNotice(null);
    setReplyKey(session.key);
    setRenameKey(session.key);
    setPicker(null);
    setError(null);
  };

  const leaveReply = () => {
    setReplyKey(null);
    setRenameKey(null);
    setQueued(false);
    setNotice(null);
    refocus();
  };

  /** Renames the session shown, or leaves its name, and goes back to the list. */
  const renamed = (s: Session, name: string | null) => {
    leaveReply();
    if (name) renameAhead(s.key, name).catch((e: unknown) => setError(plain(e)));
  };

  /** Done with a session answered from the list: the next that waits on you takes the view, saying
   *  how this one went. With none left, the panel is done. */
  const advance = (s: Session, done: string, said: string | null = null) => {
    handled.current.set(s.key, s.statusSince ?? s.lastActivity);
    const next = queueAfter(queue, s.key, handled.current)[0];
    if (!next) return replied(said);
    setNotice({ text: `${s.title}: ${done}`, error: false });
    setReplyKey(next.key);
  };

  /** On to the next without doing anything here: this one comes round again after the rest. */
  const skip = () => {
    const next = queueLeft[0];
    if (!next) return;
    setNotice(null);
    setReplyKey(next.key);
  };

  /** Marks the session answered seen, snoozes or archives it, and goes on to the next that waits on
   *  you, or back to the list it's left. */
  const dismissReply = (s: Session, done: string, act: Promise<void>) => {
    if (queued) advance(s, done);
    else leaveReply();
    act.catch((e: unknown) => {
      const why = plain(e);
      setError(why);
      setNotice({ text: `${s.title}: ${why}`, error: true });
    });
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
    openUrl(pr.url).catch((e: unknown) => reopen(() => setError(plain(e))));
  };

  const revealPr = (pr: PrItem) => {
    void hideQuick();
    void showMain({ kind: 'pr', key: pr.key });
  };

  /** Puts a PR's link on the clipboard and closes the panel, as opening the PR would. */
  const copyPrLink = (pr: PrItem) => {
    navigator.clipboard.writeText(pr.url).then(
      () => void hideQuick(),
      () => setError('Couldn’t copy the PR link.'),
    );
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

  /** Stops the turn a session is taking; its row shows the turn ending, and a stop that didn't
   *  go says why. */
  const stopItem = (s: Session) => (stoppable(s) ? [{ label: 'Stop turn', run: () => void sessionCancel(s.key).catch((e: unknown) => setError(plain(e))) }] : []);

  const rowMenu = (r: Row) => {
    if (r.kind === 'more') return;
    if (r.kind === 'suggest') {
      const sg = r.s;
      if (sg.kind === 'send' || sg.kind === 'open') {
        void showContextMenu([
          ...(sg.kind === 'send' ? [{ label: 'Send this prompt to it', run: () => send(sg.session) }] : []),
          { label: 'Open', run: () => openSession(sg.lane, sg.session) },
          { label: 'Reply here', run: () => replyTo(sg.session) },
          ...stopItem(sg.session),
          ...(renames(sg.session) ? [{ label: 'Rename…', run: () => renameIn(sg.session) }] : []),
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
        ...stopItem(r.session),
        ...(renames(r.session) ? [{ label: 'Rename…', run: () => renameIn(r.session) }] : []),
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
        'separator',
        { label: 'Copy link', run: () => copyPrLink(r.pr) },
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
  const onModel = `${startsOn ? ` on ${modelName(startsOn, models)}` : ''}${effortWord ? ` at ${effortWord.toLowerCase()} effort` : ''}`;
  const withPrompt = prompted ? ' with this prompt' : '';
  const macName = deviceInfo?.name ?? 'That Mac';
  const happensThere = !lane
    ? deviceNow
      ? `No worktrees in this project on ${macName}.`
      : !board || board.loading
        ? 'Reading worktrees…'
        : 'No worktrees in this project yet.'
    : !account || !placeNow
      ? deviceNow && remote?.error
        ? `${macName} didn’t say where its accounts can start: ${remote.error}`
        : ''
      : placeNow === 'background'
        ? inNewWorktree
          ? `Claude Code makes a new worktree of ${lane.repo} and starts ${account}${onModel} there in the background${withPrompt}. Attach a terminal any time.`
          : `${account}${onModel} starts in the background in ${lane.callsign}${withPrompt}, with no terminal. Attach one any time.`
        : placeNow === 'terminal'
        ? inNewWorktree
          ? `Claude Code makes a new worktree of ${lane.repo} and starts ${account}${onModel} there in ${terminalName}${withPrompt}.`
          : `${terminalName} starts ${account}${onModel} in ${lane.callsign}${withPrompt}.`
        : placeNow === 'codealong'
          ? `Codealong runs ${account}${onModel} in ${lane.callsign}${withPrompt}. Its panel here shows the reply, takes the next prompt and answers what it asks.`
        : placeNow === 'claude-app'
          ? `The Claude app opens a new session in ${lane.callsign}${prompted ? ' with this prompt filled in' : ''}. You send it from there.`
          : prompted
            ? `The Codex app starts a new thread in ${lane.callsign} on this prompt.`
            : `Codex opens a new thread in ${lane.callsign}. You write the prompt there.`;
  const happens = deviceNow && lane && account && placeNow ? `On ${macName}: ${happensThere}` : happensThere;
  // Only the new-thread page adds the folder as a project; a thread Codealong starts on a prompt
  // shows under the app's Recents.
  const newToCodex = placeNow === 'codex-app' && !prompted && !!lane && !!startOptionsNow && !startOptionsNow.codexProjects.includes(lane.path);

  /** Starts a session: in the worktree chosen below, or where a suggestion says. */
  const start = (over?: { lane: Lane; newWorktree: boolean }) => {
    const where = over?.lane ?? lane;
    const newTree = claudeTerminal && (over ? over.newWorktree : newWorktree);
    if (!where || !account || !placeNow) return;
    if (choice) remember(choice, placeNow);
    const spec: StartSpec = { laneId: where.id, account, place: placeNow, prompt: text, model: modelNow, effort: effortNow, worktree: newTree, todos: forTodos.map((t) => t.id) };
    const onMac = deviceInfo ? ` on ${deviceInfo.name}` : '';
    const label =
      placeNow === 'terminal' || placeNow === 'background'
        ? `Starting ${account}${onModel} in ${newTree ? `a new worktree of ${where.repo}` : where.callsign}${placeNow === 'background' ? ' in the background' : ''}${onMac}…`
        : placeNow === 'codealong'
          ? `Starting ${account}${onModel} in Codealong${onMac}…`
        : placeNow === 'claude-app'
          ? `Opening a new session in the Claude app${onMac}…`
          : `Opening a new thread in Codex${onMac}…`;
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
        reopen(() => {
          setText(kept.text);
          setTodoIds(kept.todoIds);
          setNewWorktree(kept.newWorktree);
          setError(plain(e));
        });
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
    sendAhead(session, prompt, { toDraft: false })
      .then((sent) => tellOpener(to, { phase: 'end', id, key: null, message: sent.message }))
      .catch((e: unknown) => {
        void tellOpener(to, { phase: 'end', id, key: null, message: null });
        reopen(() => {
          setText(prompt);
          setError(plain(e));
        });
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
      .catch((e: unknown) =>
        reopen(() => {
          setText(prompt);
          setError(plain(e));
        }),
      );
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
    else if (row.kind === 'more') {
      // The first of those it adds takes its place.
      setAllFor(query);
      setActiveKey(ranked[FOUND_SESSIONS]?.key ?? null);
    } else (alt ? revealSession : openSession)(row.lane, row.session);
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
      // Answering a session, only ⌘Q, ⌘H and ⌘] mean something here: the list's keys are for the
      // list. ] goes by its key's code, which layouts that type it with ⌥ keep.
      if (replying) {
        const k = e.key.toLowerCase();
        if (e.metaKey && !e.ctrlKey && !e.altKey && (k === 'q' || k === 'h')) {
          e.preventDefault();
          void hideQuick();
        } else if (e.metaKey && !e.ctrlKey && !e.shiftKey && e.code === 'BracketRight' && queued) {
          e.preventDefault();
          skip();
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
      if (k === 'p' || k === 't' || k === 'l' || (k === 'd' && macs.length > 0)) {
        e.preventDefault();
        togglePicker(k === 'p' ? 'project' : k === 'd' ? 'device' : k === 't' ? 'worktree' : 'agent');
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

  // The effort under the agent menu's models: a setting for whichever is chosen, so it stays
  // while you look through them. It goes to Claude Code only, and not to Haiku.
  const effortModel = (autoFor ? autoModelPick?.model : chosenModel) ?? defaultModel;
  const effortSteps: (string | null)[] = [null, ...efforts.map((e) => e.id)];
  // From the effort as it is, not as last drawn: a held key can repeat before the menu redraws.
  const stepEffort = (delta: -1 | 1) =>
    setEffort((e) => effortSteps[Math.min(effortSteps.length - 1, Math.max(0, Math.max(0, effortSteps.indexOf(e)) + delta))]);
  const effortFoot =
    efforts.length > 0 && accounts.some((a) => a.harness === 'claude') ? (
      <>
        <span className="ca-picker-foot-label">Effort</span>
        <div className="ca-segctl" role="radiogroup" aria-label="Effort">
          {[{ id: null, label: 'Default' }, ...efforts].map((e) => (
            <button
              key={e.id ?? 'default'}
              type="button"
              role="radio"
              aria-checked={effort === e.id}
              className={effort === e.id ? 'is-on' : undefined}
              title={e.id === null ? 'The account’s own: settings.json’s effortLevel, else the model’s' : e.id === 'max' ? 'Deepest reasoning. It can use a great many tokens; for the hardest tasks.' : undefined}
              // The filter keeps the keyboard.
              onMouseDown={(ev) => ev.preventDefault()}
              onClick={() => setEffort(e.id)}
            >
              {e.label}
            </button>
          ))}
        </div>
        <span className="ca-picker-foot-note">
          {harness && harness !== 'claude' ? `${HARNESS_WORD[harness]} picks its own` : modelFamily(effortModel) === 'haiku' ? 'Haiku takes none' : <Kbd keys={['←', '→']} />}
        </span>
      </>
    ) : null;

  const deviceSections: PickerSection[] = useMemo(() => {
    if (!devices) return [];
    const count = (short: string | null) => {
      const n = (short ? devices.lanes.filter((l) => shortOf(l.id) === short) : (board?.lanes ?? [])).filter((l) => inProject(l, current)).length;
      return `${n} worktree${n === 1 ? '' : 's'}${current === ALL_PROJECTS ? '' : ` in ${current}`}`;
    };
    const here = devices.me.name || 'This Mac';
    return [
      {
        key: 'macs',
        options: [
          { id: HERE, text: `${here} this mac`, label: here, detail: `This Mac · ${count(null)}`, lead: <Icon name="device-desktop" size={14} /> },
          ...macs.map((d) => {
            const away = !d.online ? (d.lastSeen ? `Away: last seen ${agoPhrase(d.lastSeen, now)}` : 'Away') : null;
            const off = d.online && !d.remoteControl ? 'Remote control is off there' : null;
            return {
              id: d.short,
              text: d.name,
              label: d.name,
              detail: away ?? off ?? count(d.short),
              lead: <i className={`ca-labeldot is-${d.color}`} aria-hidden="true" />,
              disabled: !!(away || off),
              title: off ? `Remote control is off on ${d.name}; turn it on there in Settings › Devices to start sessions from here.` : undefined,
            };
          }),
        ],
      },
    ];
  }, [devices, macs, board?.lanes, current, now]);

  const pickDevice = (id: string) => {
    const d = id === HERE ? null : id;
    if (d !== deviceNow) {
      // Its worktrees and accounts are its own.
      setDevice(d);
      setLaneId(null);
      setAgent(null);
      setPlace(null);
      // A Mac that didn't answer is asked again.
      if (d) setRemoteOptions(({ [d]: _, ...rest }) => rest);
    }
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
          : { icon: 'external', title: `Open “${x.title}”`, why: `${age}${sessionWhy(x.detail, x.headline, now)}`, where };
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
      placeNow === 'codealong'
        ? `Codealong runs ${who} in ${sg.lane.callsign}.`
      : placeNow === 'claude-app'
        ? `The Claude app opens a new session in ${sg.lane.callsign} with this prompt filled in.`
        : placeNow === 'codex-app'
          ? `The Codex app starts a new thread in ${sg.lane.callsign} on this prompt.`
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
    if (g === 'found') return foundSessions.length;
    return shown;
  };
  const agentLabel = autoFor ? 'Auto' : (account ?? 'No account');
  // Another Mac's accounts come a moment after it's picked, or not at all.
  const noAccounts = deviceNow && !startOptionsNow ? (remote?.error ? 'Couldn’t read its accounts' : 'Reading accounts…') : 'No accounts';
  const agentDetail = [autoFor ? account : null, modelShown, effortWord ? `${effortWord} effort` : null].filter(Boolean).join(' · ');
  const shortcut = shortcutLabel(board?.quickShortcut);

  /** A reply is on its way, or what it waited on was answered: the panel is done. The window that
   *  opened it hears what happened. */
  const replied = (message: string | null) => {
    setReplyKey(null);
    setQueued(false);
    setNotice(null);
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
            onSeen={() => dismissReply(replying.session, 'marked seen.', seenAhead([replying.session.key]))}
            onArchive={() => dismissReply(replying.session, 'archived.', archiveAhead(replying.session.key, true))}
            onSnooze={(e) => {
              const s = replying.session;
              void showContextMenu(snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: (act) => dismissReply(s, 'snoozed.', act) }).items, below(e.currentTarget));
            }}
            renaming={renameKey === replying.session.key}
            onRename={(name) => renamed(replying.session, name)}
            queue={queued ? { left: queueLeft.length, next: queueLeft[0]?.session ?? null, onNext: skip } : undefined}
            notice={notice}
            onSending={() => (queued ? advance(replying.session, 'sent.') : replied(null))}
            onSent={(sent) => {
              if (sent.message) void tellOpener(opener ?? 'main', { phase: 'end', id: Date.now(), key: null, message: sent.message });
            }}
            // Back on the session, with what you wrote in the box and why it didn't go; or, when the
            // view went on to another, it says so, and the session is back in what's left.
            onFailed={(why) => {
              const s = replying.session;
              if (shownNow.current && replyKeyNow.current && replyKeyNow.current !== s.key) {
                handled.current.delete(s.key);
                setNotice({ text: `Didn’t send to ${s.title}: ${why}`, error: true });
              } else reopen(() => setReplyKey(s.key));
            }}
            onAnswered={(said) => (queued ? advance(replying.session, said, said) : replied(said))}
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
            {!board || board.loading ? (
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
                      if (r.kind === 'more') {
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={['ca-quick-row', 'is-more', active === r.key && 'is-active'].filter(Boolean).join(' ')}
                            {...handlers}
                          >
                            <span className="ca-quick-plus">
                              <Icon name="chevron-down" size={13} stroke={2} />
                            </span>
                            <span className="ca-quick-row-main">
                              <span className="ca-quick-row-title">
                                {r.count} more {r.count === 1 ? 'session' : 'sessions'}
                              </span>
                            </span>
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
                              <span className="ca-quick-row-why">{sessionWhy(s2.detail, s2.headline, now)}</span>
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
                {deviceNow && forTodos.length > 0 && ` Todos are this Mac’s, so ${forTodos.length > 1 ? 'they aren’t' : 'it isn’t'} linked to the session.`}
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
          {macs.length > 0 && (
            <div className="ca-quick-pick">
              <button type="button" data-picker-trigger className="ca-quick-chip" title={deviceInfo ? `On ${deviceInfo.name}\nMac (⌘D)` : 'Mac (⌘D)'} aria-haspopup="dialog" aria-expanded={picker === 'device'} onClick={() => togglePicker('device')}>
                {deviceInfo ? <i className={`ca-labeldot is-${deviceInfo.color}`} aria-hidden="true" /> : <Icon name="device-desktop" size={14} />}
                <span>{deviceInfo ? deviceLabel(deviceInfo.name) : 'This Mac'}</span>
                <Icon name="chevron-down" size={12} />
              </button>
              {picker === 'device' && <Picker sections={deviceSections} value={deviceNow ?? HERE} placeholder="Mac" onPick={pickDevice} onClose={() => { setPicker(null); refocus(); }} />}
            </div>
          )}
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
            <button type="button" data-picker-trigger className="ca-quick-chip" title="Account, model and effort (⌘L)" disabled={accounts.length === 0} aria-haspopup="dialog" aria-expanded={picker === 'agent'} onClick={() => togglePicker('agent')}>
              <Icon name={autoFor ? 'sparkles' : 'user'} size={14} />
              <span>{accounts.length === 0 ? noAccounts : agentLabel}</span>
              {agentDetail && <span className="ca-quick-chip-detail">{agentDetail}</span>}
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'agent' && <Picker className="is-wide" sections={agentSections} value={agentNow} placeholder="Account or model" onPick={pickAgent} onClose={() => { setPicker(null); refocus(); }} foot={effortFoot} onStep={effortFoot ? stepEffort : undefined} />}
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

/** Whether a Claude Code start there takes the model you pick: the command line, or the adapter. */
function placeTakesModel(place: StartPlace): boolean {
  return onCommandLine(place) || place === 'codealong';
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
