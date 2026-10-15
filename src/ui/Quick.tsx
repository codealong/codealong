// The quick panel's page (quick.rs has its window): one input, and below it what you can do
// next. With nothing typed it lists the sessions that need you, then the ones whose turn ended,
// then a few todos ready to start, then the other sessions started in the last day; typed words find sessions, todos and PRs; the last row starts a
// new session with what you typed as its prompt, in the worktree, on the agent and in the app
// chosen at the bottom.
//
// A slash and the start of a name that start the text list the skills of the project's repos
// and of the accounts instead (skills.ts); one later on is words. ⏎ or ⇥ finishes the name,
// and that skill is the one a start runs, on an account whose agent reads it (one of the
// accounts that have it, for an account's own). A repo's picks where the session starts, the
// repo, in a worktree chosen as new work's is. The prompt goes with the skill's name written as
// that agent takes it.
//
// ⏎ does the selected row and ⌘⏎ always starts. ⏎ on a session or a PR shows it in Codealong,
// with its conversation or its details, and ⌥⏎ opens it where it lives (a PR on GitHub); ⌥⏎ on a
// todo shows it in Codealong. ⌘1 to ⌘9 do a row, ⌘P picks the project and ⌥⌘1 to ⌥⌘9 switch
// to one (⌥⌘0 to all of them), ⌘D picks the Mac (with others linked), ⌘T the worktree, ⌘L
// the agent, whose menu has the effort under its models (← → there), and ⌘O where it opens
// (with more than one place), ↑ in the empty input brings back the last draft and earlier
// prompts, and Escape closes. (⌘M is the Window menu's Minimize, which may take the key before
// the page sees it.)
//
// ⇥ on a session answers it, and ⏎ or ⇥ on a PR waiting on your review reviews it, in the waiting
// window (Waiting.tsx), which goes on from there to the rest that waits on you: the panel finds
// and starts, and answering is that window's.

import { reuse } from '../reuse';
import { aimAfter, aimAfterChange, aimedAt, aimedAtNothing, aimedByScroll, clickDoes, type Aim } from '../aim';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import {
  deviceStartOptions,
  getBoard,
  getDevices,
  hideQuick,
  inPreview,
  getConfig,
  keepStartPlace,
  onBoard,
  onDevices,
  onQuickHidden,
  openUrl,
  routeQuick,
  saveConfig,
  searchText,
  showMain,
  showQuick,
  showWaiting,
  startFromQuick,
  startOptions,
  tellOpener,
  todoOp,
  type QuickOpen,
} from '../api';
import { CODEALONG_PROJECT, IMPROVE_PLACEHOLDER, improvePrompt, improveRemoteHold, improveWords, remoteImproveLane, withCodealongProject } from '../improve';
import { boardAhead, boardView, observe, sendAhead, useAhead } from '../ahead';
import { filesToAttach, keepFiles, useFileDrop } from '../attachInput';
import { deviceLabel, shortOf } from '../devices';
import { useKeyPerf, usePerf } from '../perf';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { useEditorIcons } from '../editors';
import * as menus from '../menus';
import { prRefOf } from '../prLinks';
import { ago, tildePath } from '../format';
import { useLogo } from '../logo';
import { useRowDrag } from './useRowDrag';
import { prPills } from '../prStatus';
import { ALL_PROJECTS, NO_PROJECT, attachmentsNote, attentionRows, draftCodec, findPrs, findSessions, findTodos, FOUND_SESSIONS, inProject, laneBranch, laneGroups, macUnavailable, MAX_ROWS, nextDraft, primaryRunningWords, projectCounts, queryWords, readyTodos, recallList, recentRows, shortcutLabel, startTitle, stillSaid, TEXT_SEARCH_MIN, todosStayNote, worktreeCount, type PrRow, type QuickDraft, type ReviewRow, type RowGroup, type SessionRow, type TodoRow, withPrompt as historyWith } from '../quick';
import {
  agentChipWords,
  isAuto,
  isGroup,
  keepPlaceFailed,
  keepPlaceFor,
  KEEP_PLACE_NOTICE,
  newWorktreeTitle,
  onModelWords,
  remember,
  START_OPTIONS_NOTICE,
  startHappens,
  startingLabel,
  startOptionsFailed,
  startTitleWords,
  startWhereWords,
  withChosen,
} from '../start';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { completed, mentionAt, mentionsIn, namedBy, repoLanes, skillLeftOutWords, skillOffers, skillRows, skillStart, skillWhere, writtenFor, type Mention, type SkillRow } from '../skills';
import { useSkills } from '../useSkills';
import { alreadyOnWords, duplicateOf, rankedBy, resolveSuggestedAgent, suggestions, suggestionWords, sureness, todoTitle, worthRouting, type Suggestion } from '../route';
import { dueWords } from '../due';
import { laneWaits, liveOrLatest, sessionHomes, sessionWhy } from '../lanes';
import { blockedBy, mergeAttachments, newTodo, primaryHolds, snippet, splitAttachments, todoAgent, todoItem, todoProjects, todoPrompt, todosPrompt, withAttachments } from '../todos';
import type { Attachment, Board, DesktopPinned, DevicesView, Lane, PrItem, Route, Session, StartOptions, StartPlace, StartSpec, TextFound } from '../types';
import { AttachmentStrip } from './DescriptionEditor';
import { Picker, type PickerOption, type PickerSection } from './Picker';
import { agentMenuSections, EffortFoot, placeIcon, placeSections } from './StartControls';
import { Button, Chip, cx, Icon, Kbd, Mark, STATUS_WORD, SessionGlyph, StatusGlyph } from './primitives';
import { projectForDigit, projectKey } from './ProjectSwitcher';
import { usePrLinks } from './PrLink';
import { PrGlyph } from './PrsView';
import { useTheme } from './ThemeSwitch';
import { Tip } from './Tip';
import { TodoGlyph } from './TodoBits';
import { useNow } from './useNow';
import { errorText, type Report } from '../report';
import { openWhere, where } from '../open';
import { asText, flag, readStored, strings, useStored, writeStored } from '../stored';
import { useLatest } from '../latest';
import { useStartChoice, useStartPicks } from '../useStartChoice';
import { MARGIN, useQuickSize } from './useQuickSize';

declare global {
  interface Window {
    /** Opens the panel for what Codealong asks, with the board, and returns its height once it's
     *  laid out (quick.rs calls it). */
    __quickOpen?: (context: QuickOpen | null, board: Board | null) => number;
    /** An open that came before the page was up, for the page to do once it is. */
    __quickPending?: [QuickOpen | null, Board | null];
  }
}

const PROJECT_KEY = 'codealong.quick.project';
const HISTORY_KEY = 'codealong.quick.history';
/** What you'd typed and not acted on, so a relaunch (a new build) doesn't lose it. */
const DRAFT_KEY = 'codealong.quick.draft';
/** Whether a search lists archived sessions too, under the others. */
const ARCHIVED_KEY = 'codealong.quick.archived';
/** The input grows to this many lines, then scrolls. */
const INPUT_LINES = 8;
const COMPOSE = 'compose';
/** How long a read of where accounts can start is used without waiting for a new one. */
const OPTIONS_FOR_MS = 60_000;
/** How long after another Mac didn't say where its accounts can start it's asked again. */
const RETRY_MS = 5_000;
/** The device menu's id for this Mac; the others go by their short ids, which are hex. */
const HERE = 'here';

type PickerKind = 'project' | 'device' | 'worktree' | 'agent' | 'place';

const GROUP_WORD: Record<RowGroup, string> = { suggested: 'Suggested', skills: 'Skills', needs: 'Needs you', reviews: 'Review requested', unread: 'Your turn', working: 'Working', found: 'Sessions', todos: 'Todos', prs: 'PRs', ready: 'Ready todos', recent: 'Last 24 hours' };

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

type Row = SessionRow | ReviewRow | TodoRow | PrRow | SuggestRow | MoreRow | SkillRow;

/** Jev's suggestions as rows. Jev knows this Mac's worktrees, so it suggests starting only while a
 *  session would start here, not on another Mac. */
function suggestRows(route: Route | null, board: Board | null, elsewhere: boolean): SuggestRow[] {
  if (!board || !route) return [];
  return suggestions(route, board)
    .filter((sg) => !elsewhere || sg.kind !== 'start')
    .map((sg) => ({ kind: 'suggest', key: sg.key, group: 'suggested', s: sg }));
}

/** The most sessions a search lists once you ask for all of them. */
const FOUND_ALL = 40;

/** How long typing has to pause before what's typed goes to Jev. */
const ROUTE_AFTER_MS = 350;
/** How long typing has to pause before what's typed is looked for in what sessions said. */
const TEXT_AFTER_MS = 150;

/** What a found todo's row says under its title: when it's due, then what it waits for, else the
 *  start of its description, else how many sessions it has. */
function todoWhy(r: TodoRow, now: number): string {
  const t = r.item.todo;
  const due = t.due && t.status !== 'done' ? dueWords(t.due, now)?.text : undefined;
  return [due, todoState(r)].filter(Boolean).join(' · ');
}

function todoState(r: TodoRow): string {
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
/** What a result pins to the desktop when it's dragged out of the panel: its session, todo or
 *  PR. Null for what has no card: a session on another device or archived, a start, more results. */
function deskPin(r: Row | undefined): DesktopPinned | null {
  const session = r?.kind === 'session' ? r : r?.kind === 'suggest' && (r.s.kind === 'send' || r.s.kind === 'open') ? r.s : null;
  if (session) return session.lane.device || session.session.archived ? null : { ref: 'session', key: session.session.key };
  if (r?.kind === 'todo') return { ref: 'todo', key: r.item.todo.id };
  if (r?.kind === 'pr' || r?.kind === 'review') return { ref: 'pr', key: r.pr.key };
  return null;
}

/** What the pointer holds while it drags a result. */
function deskTitle(r: Row | undefined): string {
  if (r?.kind === 'session') return r.session.title;
  if (r?.kind === 'suggest' && (r.s.kind === 'send' || r.s.kind === 'open')) return r.s.session.title;
  if (r?.kind === 'todo') return r.item.todo.title || 'Untitled';
  if (r?.kind === 'pr' || r?.kind === 'review') return `#${r.pr.number} ${r.pr.title}`;
  return '';
}

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

/** What Auto does when it starts a session, on its line in the agent menu. */
const AUTO_STARTS =
  `Starts an account of the first account’s harness while one has room (or from Auto’s routing group in Settings › Automation), else another. Among them, the one whose weekly limit resets soonest, so its room isn't lost, ` +
  `else the one whose fullest limit is least full, on its own default model; when that model's own limit is nearly used, on the next best. ` +
  `An account at its usage cap (90% by default) has no room. Decided when you start.`;

export function Quick() {
  const [engineBoard, setBoard] = useState<Board | null>(null);
  // Changes made in any window that the engine hasn't shown yet (ahead.ts), on top of the board.
  const ahead = useAhead();
  const board = useMemo(() => (engineBoard ? boardAhead(engineBoard, ahead) : null), [engineBoard, ahead]);
  // For the menus: the editors' icons, and what's on the desktop.
  const icons = useEditorIcons(board?.apps);
  const onDesktop = menus.useDesktopPins();
  const [options, setOptions] = useState<StartOptions | null>(null);
  // Up (or about to be): the page follows the board only then.
  const [shown, setShown] = useState(false);
  // Counts opens; each one is answered with the page's height once it's laid out.
  const [opens, setOpens] = useState(0);
  // How tall the window can be on its screen; the card keeps within it and the list scrolls.
  const [room, setRoom] = useState<number | null>(null);
  const [opener, setOpener] = useState<string | null>(null);
  const [project, setProject] = useState<string>(() => readStored(PROJECT_KEY, asText, ALL_PROJECTS));
  const [text, setText] = useState('');
  const [attached, setAttached] = useState<Attachment[]>([]);
  const [laneId, setLaneId] = useState<string | null>(null);
  // The account or Auto, model, effort and place picked for a new session, and whether it gets a
  // new worktree (useStartChoice.ts).
  const startPicks = useStartPicks();
  const { effort, setEffort, newWorktree, setNewWorktree, setAgent, setPlace } = startPicks;
  // The Mac a new session starts on, by its short id; null is this one.
  const [device, setDevice] = useState<string | null>(null);
  const [devices, setDevices] = useState<DevicesView | null>(null);
  // Where accounts can start on your other Macs, asked of each once it's chosen.
  const [remoteOptions, setRemoteOptions] = useState<Record<string, { at: number; options: StartOptions | null; error: string | null }>>({});
  // The todos a new session is for: one, or several started together on one prompt.
  const [todoIds, setTodoIds] = useState<string[]>([]);
  // Improve Codealong (improve.ts): the pictures of the windows the engine took as it opened the
  // panel, which go into the prompt; null when the panel is open for anything else.
  const [improve, setImprove] = useState<{ shots: Attachment[] } | null>(null);
  // Opened by the chat shortcut: on Codealong's chats, which the worktree picker calls No project,
  // until another is picked, and the hint names that shortcut.
  const [asChat, setAsChat] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What a menu's action did (copied, closed), for a moment.
  const [info, setInfo] = useState<string | null>(null);
  useEffect(() => {
    if (!info) return;
    const id = window.setTimeout(() => setInfo(null), 4000);
    return () => window.clearTimeout(id);
  }, [info]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [picker, setPicker] = useState<PickerKind | null>(null);
  const now = useNow(30_000);
  const input = useRef<HTMLTextAreaElement>(null);
  const inputBox = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const textNow = useLatest(text);
  const attachedNow = useLatest(attached);
  const fullPrompt = withAttachments(text, attached);
  const fullPromptNow = useLatest(fullPrompt);
  const loadPrompt = (raw: string): string => {
    const next = splitAttachments(raw);
    setText(next.text);
    setAttached(next.attachments);
    return next.text;
  };
  const clearPrompt = () => {
    setText('');
    setAttached([]);
  };
  const cachedOptions = useRef<{ at: number; options: StartOptions } | null>(null);
  // Jev's answers by project and text, so going back to what you typed doesn't ask again. Each
  // open starts afresh: sessions come and go.
  const routes = useRef(new Map<string, Route>());
  // What you'd typed when you last closed the panel without acting on it. Every open starts empty,
  // so the list is what needs you and ⏎ goes to it; ↑ in the empty input brings the draft back.
  // It's kept on every change as the next open would leave it (below), so a relaunch brings it
  // back too.
  const [draft, setDraft] = useState<QuickDraft | null>(() => readStored(DRAFT_KEY, draftCodec, null));
  // Prompts you sent, started or saved, newest first, kept across restarts. ↑ walks back through
  // the draft and then these; `recall` is where it is, and holds while the text is what it
  // brought back.
  const [history, setHistory] = useStored<string[]>(HISTORY_KEY, strings, []);
  const [withArchived, setWithArchived] = useStored(ARCHIVED_KEY, flag, false);
  const [recall, setRecall] = useState<number | null>(null);
  const keepPrompt = (prompt: string) => setHistory((h) => historyWith(h, prompt));
  const todoIdsNow = useLatest(todoIds);
  // While an open lays the panel out: 'script' when Codealong's script opens it (its height goes
  // back as the script's result), 'page' when the page opened it itself (it sends its height).
  const opening = useRef<'script' | 'page' | null>(null);
  // The height last reported for this open, by the open or by a fit.
  const reported = useRef(0);
  // When a board push arrived, for the perf log.
  const arrived = useRef<number | null>(null);
  // The row the pointer was last moved onto, so a click does only a row you aimed at (aim.ts).
  const aim = useRef<Aim | null>(null);

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
          clearNotice(START_OPTIONS_NOTICE);
        })
        // A failed read leaves the copy there is, and says so until a read works.
        .catch((e: unknown) => reportNotice(startOptionsFailed(e)))
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
        // Improve Codealong (improve.ts) opens on Codealong's own project and worktree, as the
        // board has them, with the pictures the engine took of the windows.
        const improving = ctx.improve && typeof ctx.improve === 'object' ? ctx.improve : null;
        setImprove(improving);
        improveFollowed.current = false;
        setAsChat(!!ctx.chat);
        const at = improving ? (b?.improve?.laneId ?? null) : (ctx.laneId ?? null);
        setProject(improving && b?.improve?.project ? b.improve.project : (ctx.project ?? readStored(PROJECT_KEY, asText, ALL_PROJECTS)));
        setLaneId(at);
        setTodoIds(ctx.todos ?? (ctx.todo ? [ctx.todo] : []));
        const typed = fullPromptNow.current;
        const typedFor = todoIdsNow.current;
        setDraft((d) => nextDraft(typed, typedFor, d));
        loadPrompt(ctx.prompt ?? '');
        setRecall(null);
        setAgent(null);
        setEffort(null);
        setPlace(null);
        setDevice(at ? shortOf(at) : null);
        setNewWorktree(false);
        setActiveKey(null);
      }
      if (b) {
        setBoard(b);
        // A todo asked for without its prompt (by the floating bar, which doesn't have it)
        // starts the way the Todo view starts it: as its prompt, in its worktree.
        const ids = ctx.todos ?? (ctx.todo ? [ctx.todo] : []);
        const ts = !ctx.keep && ctx.prompt === undefined ? ids.flatMap((id) => b.todos.find((x) => x.id === id) ?? []) : [];
        if (ts.length > 0) {
          const homes = sessionHomes(b.lanes);
          const now = Date.now();
          loadPrompt(todosPrompt(ts, (t) => primaryHolds(t, homes, now)));
          const laneById = new Map(b.lanes.map((l) => [l.id, l]));
          if (!ctx.laneId) setLaneId(ts.map((t) => todoItem(t, laneById, homes, new Map()).lane?.id).find(Boolean) ?? null);
        }
      }
      // Whatever the pointer rests on as the panel shows wasn't aimed at.
      aim.current = aimedAtNothing(aim.current);
      setShown(true);
      setOpens((n) => n + 1);
    };
    const refresh = (b: Board | null) => {
      if (!cachedOptions.current || Date.now() - cachedOptions.current.at >= OPTIONS_FOR_MS) readOptions();
      // Your other Macs, for starting there, when there are any; while the panel is up it
      // follows them.
      if (b?.devices?.enabled)
        getDevices()
          .then((d) => {
            if (d) setDevices(d);
            clearNotice('quick-devices');
          })
          .catch((e: unknown) => reportNotice(failedNotice('quick-devices', 'Can’t read your other devices', 'The quick panel couldn’t read your other Macs, so it can’t start sessions there', e)));
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
          setError(errorText(e));
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
    // Once: the refs and setters are the same objects on every render.
  }, [fullPromptNow, todoIdsNow, setAgent, setEffort, setPlace, setNewWorktree]);

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
  useTheme(theme);
  const logoStatus = useLogo(board?.logoStatus);

  // The input grows with what you write.
  useLayoutEffect(() => {
    const el = input.current;
    if (!el) return;
    el.style.height = 'auto';
    const line = parseFloat(getComputedStyle(el).lineHeight) || 22;
    el.style.height = `${Math.min(el.scrollHeight, Math.ceil(line * INPUT_LINES))}px`;
  }, [text, opens]);

  const refocus = () => window.requestAnimationFrame(() => input.current?.focus());

  /* ---- Where ---- */

  const projects = useMemo(() => board?.projects ?? [], [board?.projects]);
  // The board lists no projects until it's read, so the one picked holds until then.
  const current = project === ALL_PROJECTS || projects.some((p) => p.name === project) || !board || board.loading ? project : ALL_PROJECTS;
  const pickProject = (name: string) => {
    setProject(name);
    setActiveKey(null);
    setPicker(null);
    writeStored(PROJECT_KEY, asText, name);
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
      .catch((e: unknown) => setRemoteOptions((r) => ({ ...r, [d]: { at: Date.now(), options: r[d]?.options ?? null, error: errorText(e) } })));
  }, [shown, deviceNow, remote]);
  const startOptionsNow = deviceNow ? (remote?.options ?? null) : options;

  // Where a session can start: the project's worktrees on that Mac, and the one a window asked
  // for. Codealong's chats are apart from them, as No project, whatever the project.
  const { lanes, chatLane } = useMemo(() => {
    const all = deviceNow ? (devices?.lanes ?? []).filter((l) => shortOf(l.id) === deviceNow) : (board?.lanes ?? []);
    const worktrees = all.filter((l) => !l.startsChats);
    const mine = worktrees.filter((l) => inProject(l, current));
    const extra = laneId && !mine.some((l) => l.id === laneId) ? worktrees.filter((l) => l.id === laneId) : [];
    return { lanes: [...mine, ...extra], chatLane: all.find((l) => l.startsChats) ?? null };
  }, [board?.lanes, devices?.lanes, deviceNow, current, laneId]);
  const groups = useMemo(() => laneGroups(lanes, board?.pinnedWorktrees ?? []), [lanes, board?.pinnedWorktrees]);
  // Without one asked for, the first on the list: a pinned one, else the latest. Opened by the
  // chat shortcut, the chats lane, once the board has it: a panel opened before the first scan
  // finished gets it with the board that has it.
  // Improving Codealong with no source on the board goes to no worktree by default (the note
  // says why), never to the project's first: that would start it in some other repo.
  const lane = lanes.find((l) => l.id === laneId) ?? (chatLane && (chatLane.id === laneId || (asChat && !laneId)) ? chatLane : null) ?? (improve ? null : (groups[0]?.lanes[0] ?? null));

  const shownNow = useLatest(shown);

  /* ---- A skill, which picks its repo ---- */

  // A slash and a name being typed at the start of the text list the skills of the project's
  // repos on this Mac and the accounts' own; the skill it names whole is the one a start runs,
  // and a repo's is where. A todo's prompt is words, and another Mac's folders and accounts
  // aren't read from here.
  const mentions = useMemo(() => (todoIds.length === 0 && !deviceNow ? mentionsIn(text) : []), [text, todoIds.length, deviceNow]);
  // Where the caret is, moved as the text is typed, the caret moved, or a name finished.
  const [caret, setCaret] = useState(0);
  const typing: Mention | null = mentions.length > 0 ? mentionAt(text, Math.min(caret, text.length)) : null;
  const repos = useMemo(() => repoLanes(lanes), [lanes]);
  const repoPaths = useMemo(() => repos.map((l) => l.path), [repos]);
  const accountNames = useMemo(() => (options?.accounts ?? []).map((a) => a.name), [options?.accounts]);
  const listedSkills = useSkills(repoPaths, accountNames, shown && mentions.length > 0);
  const typingName = typing?.name ?? null;
  const skillsFound = useMemo(() => (typingName === null ? [] : skillRows(listedSkills, repos, typingName, false, MAX_ROWS)), [listedSkills, repos, typingName]);
  // The name that starts the text, when it's a skill of a repo or an account, with its rows (one
  // a repo that has it). A full stop that ends the sentence isn't in it (skills.ts `namedBy`).
  const named = useMemo(() => {
    const rowsFor = (name: string) => skillRows(listedSkills, repos, name, true, MAX_ROWS);
    for (const m of mentions) {
      const n = namedBy(m, (name) => rowsFor(name).length > 0);
      if (n) return { mention: n, rows: rowsFor(n.name) };
    }
    return null;
  }, [mentions, listedSkills, repos]);
  // The row picked says which repo's, for a skill several have; else the likeliest.
  const [skillKey, setSkillKey] = useState<string | null>(null);
  const skillNow = named ? (named.rows.find((r) => r.key === skillKey) ?? named.rows[0]) : null;
  // The list is the skills' while a name is typed or a skill of several repos waits on its repo;
  // what has a slash but names no skill ("/tmp is full") is words like any other.
  const choosing = !!named && named.rows.length > 1 && !named.rows.some((r) => r.key === skillKey);
  const slash = !!typing || !!skillNow;
  const skillsListed = typing ? skillsFound : choosing ? named.rows : [];
  // The session goes where new work in the skill's repo goes; what was chosen before comes back
  // when the skill's gone from the input. The worktree menu still changes it after.
  const skillNowKey = skillNow?.key ?? null;
  const beforeSkill = useRef<{ laneId: string | null; newWorktree: boolean; put: string } | null>(null);
  const lanesNow = useLatest(lanes);
  const laneIdNow = useLatest(laneId);
  const newWorktreeNow = useLatest(newWorktree);
  const skillLaneNow = useLatest(skillNow?.lane ?? null);
  useEffect(() => {
    const repo = skillLaneNow.current;
    if (!skillNowKey || !repo) {
      // Only while the worktree is still the one the skill put there: an open that asks for a
      // worktree, or one you picked since, stays.
      if (beforeSkill.current && laneIdNow.current === beforeSkill.current.put) {
        setLaneId(beforeSkill.current.laneId);
        setNewWorktree(beforeSkill.current.newWorktree);
      }
      beforeSkill.current = null;
      return;
    }
    const to = skillStart(repo, lanesNow.current);
    beforeSkill.current = { laneId: laneIdNow.current, newWorktree: newWorktreeNow.current, ...beforeSkill.current, put: to.lane.id };
    setLaneId(to.lane.id);
    setNewWorktree(to.newWorktree);
  }, [skillNowKey, skillLaneNow, lanesNow, laneIdNow, newWorktreeNow, setNewWorktree]);
  // Only accounts whose agent reads the skill are offered (asked 2026-10-04), and for an
  // account's own skill, only the accounts that have it.
  const optionsForStart = useMemo(
    () => (skillNow && startOptionsNow ? { ...startOptionsNow, accounts: startOptionsNow.accounts.filter((a) => skillOffers(skillNow, a)) } : startOptionsNow),
    [skillNow, startOptionsNow],
  );
  const skillLeftOut = skillNow && startOptionsNow ? skillLeftOutWords(skillNow, startOptionsNow.accounts) : null;

  /* ---- Who ---- */

  // Another Mac's usage comes with its feed.
  const usage = useMemo(() => (deviceNow ? (deviceInfo?.usage ?? []) : (board?.usage ?? [])), [deviceNow, deviceInfo?.usage, board?.usage]);
  const allLanes = useMemo(() => (deviceNow ? (devices?.lanes ?? []).filter((l) => shortOf(l.id) === deviceNow) : (board?.lanes ?? [])), [deviceNow, devices?.lanes, board?.lanes]);
  // The account, model, effort and place a new session starts with (useStartChoice.ts).
  const {
    accounts,
    groups: routingGroups,
    models,
    efforts,
    pick,
    agents,
    agentNow,
    autoFor,
    account,
    harness,
    places,
    placeNow,
    makesWorktree: harnessMakesWorktree,
    inNewWorktree: newWorktreePicked,
    modelNow,
    startsOn,
    effortModel,
    effortNow,
    effortWord,
    modelShown,
    choice,
    pickAgent: pickAgentLine,
    pickPlace,
    stepEffort,
  } = useStartChoice(startPicks, optionsForStart, usage, now, allLanes);
  // A chat has no repo to make a worktree of.
  const makesWorktree = harnessMakesWorktree && !lane?.startsChats;
  const inNewWorktree = newWorktreePicked && makesWorktree;

  /* ---- What for ---- */

  const forTodos = useMemo(() => (board ? todoIds.flatMap((id) => board.todos.find((t) => t.id === id) ?? []) : []), [board, todoIds]);
  const todo = forTodos[0] ?? null;
  // What each waits on that isn't done, leaving out the others: started together, they're done
  // together.
  const blocked = board ? forTodos.map((t) => ({ t, on: blockedBy(t, board.todos).filter((w) => !todoIds.includes(w.id)) })).filter((b) => b.on.length > 0) : [];
  const waitsOn = [...new Map(blocked.flatMap((b) => b.on).map((w) => [w.id, w])).values()];
  const marked = forTodos.filter((t) => t.waiting);
  // The todos whose primary session is running, with it: a new session would work alongside it.
  const held = useMemo(() => {
    if (!board) return [];
    const laneById = new Map(board.lanes.map((l) => [l.id, l]));
    const homes = sessionHomes(board.lanes);
    return forTodos.flatMap((t) => {
      const at = todoItem(t, laneById, homes, new Map()).primary;
      return at ? [{ todo: t, at }] : [];
    });
  }, [board, forTodos]);
  // Improve Codealong's pictures of the windows, shown with the attachments and sent after them:
  // with a start, a send to a session, a todo saved.
  const shots = improve?.shots ?? [];
  const withShots = (words: string) => (improve ? improvePrompt(words, improve.shots) : words);
  // Improving with Codealong's source on none of this Mac's worktrees: the board says it's in
  // none of your projects (improve.ts), and a project can be made for it, after which the board
  // lists its worktrees and the panel moves onto them, once; or another of your Macs has it,
  // which the note says. The panel never starts there by itself: the pictures are this Mac's
  // files, which a session there couldn't read, so a start there with them is held.
  const remoteSource = improve && !board?.improve?.laneId ? remoteImproveLane(devices?.lanes ?? []) : null;
  const improveNote = improve ? improveWords(board?.improve, !board || board.loading, remoteSource) : null;
  const improveSource = board?.improve?.source ?? null;
  // The source's worktree is taken from the board once per open: a worktree or Mac you pick
  // after that stands.
  const improveFollowed = useRef(false);
  useEffect(() => {
    if (!improve || improveFollowed.current || !board?.improve?.laneId) return;
    improveFollowed.current = true;
    setLaneId(board.improve.laneId);
    setDevice(null);
    if (board.improve.project) setProject(board.improve.project);
  }, [improve, board?.improve?.laneId, board?.improve?.project]);
  const addCodealongProject = () => {
    if (!improveSource) return;
    const source = improveSource;
    setError(null);
    getConfig()
      .then((view) => saveConfig(withCodealongProject(view.config, source)))
      .then(() => setInfo(`Added the ${CODEALONG_PROJECT} project; its worktrees come onto the board in a moment.`))
      .catch((e: unknown) => setError(errorText(e)));
  };

  // Attachments go into the strip below the input rather than as markdown in the text box.
  const addAttachments = (kept: Attachment[]) => {
    setAttached((prev) => mergeAttachments(prev, kept));
    refocus();
  };
  useFileDrop(inputBox, addAttachments, setError);

  /* ---- The list ---- */

  // With a todo to start, the input holds its prompt, not a search: only the new session is
  // listed.
  const prompted = text.trim().length > 0 || attached.length > 0;
  const query = todo || slash ? '' : text.trim();
  // With nothing typed or attached, a few todos ready to start go under the sessions, which leave them room.
  const listing = !query && attached.length === 0 && !todo && !slash;
  const readyList = useMemo(() => (board && listing ? readyTodos(board, current) : null), [board, current, listing]);
  const attention = useMemo(() => (board && listing ? attentionRows(board, current, MAX_ROWS - (readyList?.rows.length ?? 0)) : null), [board, current, listing, readyList]);
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
        // What matches by title still shows, and the search is asked again as typing goes on.
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
  const foundSessions = useMemo(() => (board && query ? findSessions(board, current, query, FOUND_ALL, said, withArchived ? 'with' : 'without') : []), [board, current, query, said, withArchived]);
  // How many archived sessions match, for the toggle that lists them or leaves them out.
  const archivedFound = useMemo(() => (board && query ? findSessions(board, current, query, FOUND_ALL, said, 'only').length : 0), [board, current, query, said]);
  // A search lists its first few sessions, so todos and PRs fit below; a row under them lists
  // the rest, until what's typed changes.
  const [allFor, setAllFor] = useState<string | null>(null);
  const showAll = allFor === query;
  const foundTodos = useMemo(() => (board && query ? findTodos(board, current, query) : []), [board, current, query]);
  const foundPrs = useMemo(() => (board && query ? findPrs(board, current, query) : []), [board, current, query]);

  // With routing on, what you type (and any pictures attached to it) goes to Jev once you pause,
  // and its suggestions lead the list, so ⏎ does the likeliest. A word or two alone is a search
  // and isn't sent.
  const routePrompt = query ? withShots(withAttachments(query, attached)) : '';
  const routeFor = board?.jev?.quick_panel && !todo && worthRouting(routePrompt) ? `${current}\n${routePrompt}` : null;
  const [routed, setRouted] = useState<{ for: string; route: Route } | null>(null);
  const [routeError, setRouteError] = useState<{ for: string; message: string } | null>(null);
  useEffect(() => {
    if (!routeFor) {
      // Nothing goes to Jev now, so the last answer stands in for no later one.
      setRouted(null);
      return;
    }
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
        .catch((e: unknown) => live && setRouteError({ for: routeFor, message: errorText(e) }));
    }, ROUTE_AFTER_MS);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [routeFor]);
  const route = routeFor && routed?.for === routeFor ? routed.route : null;
  const routeProblem = routeFor && routeError?.for === routeFor ? routeError.message : null;
  const routing = !!routeFor && !route && !routeProblem;
  // While the next answer is worked out, the last one's suggestions in this project stay where
  // they are, dimmed and doing nothing: they were for what you'd typed before. Taking them away at
  // every key moved every row under them twice each time typing paused.
  const lastRoute = routing && routed && routed.for.slice(0, routed.for.indexOf('\n')) === current ? routed.route : null;
  const suggested = useMemo(() => suggestRows(route, board, !!deviceNow), [board, route, deviceNow]);
  const lastSuggested = useMemo(() => suggestRows(lastRoute, board, !!deviceNow), [board, lastRoute, deviceNow]);

  // What Jev said beyond where it goes: a session that may already be on it.
  const duplicate = board ? duplicateOf(route, board) : null;
  const alreadyOn = alreadyOnWords(duplicate);

  // The Suggested group is there while what's typed goes to Jev, and only grows meanwhile: an
  // answer with fewer suggestions than the last leaves room where they were, so the rows under it
  // stay put. It lets go once what's typed no longer goes to Jev. Laid out before the panel's
  // height is measured, below.
  const suggestedGroup = useRef<HTMLDivElement>(null);
  const suggestedTall = useRef(0);
  useLayoutEffect(() => {
    const el = suggestedGroup.current;
    if (!el) {
      suggestedTall.current = 0;
      return;
    }
    el.style.minHeight = '';
    suggestedTall.current = Math.max(suggestedTall.current, el.offsetHeight);
    el.style.minHeight = `${suggestedTall.current}px`;
  });

  // The panel's height, as the list may change it while you type, told to quick.rs
  // (useQuickSize.ts).
  useQuickSize({ card, list, scroller, input, text: fullPrompt, opens, shown, room, answered: !routing && (!textFor || !!textFound), opening, reported });

  // The last answer orders the sessions found too while the next is worked out, so the ones it
  // put first don't drop back at every key.
  const ranked = rankedBy(foundSessions, route ?? lastRoute);
  const moreRow: MoreRow[] = !showAll && ranked.length > FOUND_SESSIONS ? [{ kind: 'more', key: 'more:found', group: 'found', count: ranked.length - FOUND_SESSIONS }] : [];
  const rows: Row[] = slash ? skillsListed : query ? [...suggested, ...(showAll ? ranked : ranked.slice(0, FOUND_SESSIONS)), ...moreRow, ...foundTodos, ...foundPrs] : [...(attention?.rows ?? []), ...(readyList?.rows ?? []), ...(recent?.rows ?? [])];
  /** Lists archived sessions in searches, or leaves them out, from now on (Sessions' heading has
   *  the box, ⇧⌘A presses it). They come after all the others, so turned on, the search lists all
   *  it finds, and the ones just asked for show. */
  const toggleArchived = () => {
    if (!withArchived) setAllFor(query);
    setWithArchived(!withArchived);
  };
  const keys = [...rows.map((r) => r.key), COMPOSE];
  // Rows that move by themselves end what a scroll of yours aimed at (aim.ts): where each sits in
  // the list, which scrolling doesn't change, is compared once the list and its held heights
  // above are laid out. Rows keep their keys as a heading or a note comes in over them.
  const rowsAt = useRef('');
  useLayoutEffect(() => {
    const at = Array.from(list.current?.querySelectorAll<HTMLElement>('[data-row]') ?? [], (el) => `${el.dataset.row}@${el.offsetTop}`).join(' ');
    if (at === rowsAt.current) return;
    rowsAt.current = at;
    aim.current = aimAfterChange(aim.current);
  });
  // A result dragged out of the panel is pinned to the desktop: a session, a todo, a PR, or the
  // session a suggestion is about (useRowDrag.tsx). Nothing here is put in order.
  const rowsNow = useLatest(rows);
  const drag = useRowDrag({
    desktop: (key) => deskPin(rowsNow.current.find((r) => r.key === key)),
    label: (key) => deskTitle(rowsNow.current.find((r) => r.key === key)),
  });
  const recallable = useMemo(() => recallList(draft?.text ?? null, history), [draft, history]);
  useEffect(() => {
    writeStored(DRAFT_KEY, draftCodec, nextDraft(fullPrompt, todoIds, draft));
  }, [fullPrompt, todoIds, draft]);
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
  const openSession = (lane: Lane, session: Session) => openHere(lane, session.openTarget);

  /** Opens one of a worktree's targets where it lives (open.ts): the panel puts itself away,
   *  and comes back as it was if the open fails. */
  const openHere = (lane: Lane, target: string) => {
    const kept = fullPromptNow.current;
    openWhere(where(lane, target), {
      from: 'panel',
      report: {
        error: (message) =>
          shownNow.current
            ? setError(message)
            : reopen(() => {
                loadPrompt(kept);
                setError(message);
              }),
        info: setInfo,
      },
      before: () => {
        clearPrompt();
        setActiveKey(null);
        setError(null);
        void hideQuick();
      },
    });
  };

  const revealSession = (lane: Lane, session: Session) => {
    void hideQuick();
    void showMain({ kind: 'session', laneId: lane.id, key: session.key });
  };

  /** What picking a session does: Codealong shows it, with its conversation, rather than the
   *  terminal or app it runs in. The search that found it is done with, as when it opens there. */
  const showSession = (lane: Lane, session: Session) => {
    clearPrompt();
    setActiveKey(null);
    setError(null);
    revealSession(lane, session);
  };

  /** Answers a session, or reviews a PR waiting on your review, in the waiting window
   *  (Waiting.tsx), which goes on from it to the rest that waits on you; `rename` shows a session
   *  with its name ready to change. The search that found it is done with, as when it opens
   *  where it lives. */
  const answerIn = (key: string, rename = false) => {
    clearPrompt();
    setActiveKey(null);
    setError(null);
    void hideQuick();
    void showWaiting({ key, rename });
  };

  /** The session ⏎ opens for a todo, as in the main window: its primary one while that runs,
   *  else the live one, else the latest. */
  const todoSession = (r: TodoRow) => todoAgent(r.item);

  /** Makes the new session the todo's: its prompt, its worktree, and linked to it. It starts
   *  when you say so. */
  const prepareTodo = (r: TodoRow) => {
    setTodoIds([r.item.todo.id]);
    loadPrompt(todoPrompt(r.item.todo, board ? primaryHolds(r.item.todo, sessionHomes(board.lanes), Date.now()) : undefined));
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
    if (s) showSession(s.lane, s.session);
    else prepareTodo(r);
  };

  const revealTodo = (r: TodoRow) => {
    void hideQuick();
    void showMain({ kind: 'todo', id: r.item.todo.id });
  };

  /** A PR opens where it lives: on GitHub. */
  const openPr = (pr: PrItem) => openLink(pr.url);

  /** A web link opens in the browser, with the panel out of the way. */
  const openLink = (url: string) => {
    void hideQuick();
    openUrl(url).catch((e: unknown) => reopen(() => setError(errorText(e))));
  };

  const revealPr = (pr: PrItem) => {
    void hideQuick();
    void showMain({ kind: 'pr', key: pr.key });
  };

  /** What picking a PR does: Codealong shows it, as a session is shown, rather than GitHub. The
   *  search that found it is done with. */
  const showPr = (pr: PrItem) => {
    clearPrompt();
    setActiveKey(null);
    setError(null);
    revealPr(pr);
  };

  /** The latest session that worked on a PR, where it's listed. */
  const prSession = (pr: PrItem): { lane: Lane; session: Session } | null => {
    const found = pr.sessions.flatMap((x) => {
      const lane = board?.lanes.find((l) => l.id === x.lane);
      const session = lane?.sessions.find((s) => s.key === x.key);
      return lane && session ? [{ lane, session }] : [];
    });
    return liveOrLatest(found);
  };

  /** How the menus say how things went: under the input. */
  const menuReport: Report = { error: setError, info: setInfo };

  /** What the panel can do to a session (menus.ts), for its menu. Reply here and Rename… go to
   *  the waiting window. */
  const quickCan = (lane: Lane, s: Session): menus.SessionCan => ({
      report: menuReport,
      open: openHere,
      terminal: board?.apps.terminal ?? 'Terminal',
      reveal: () => revealSession(lane, s),
      reply: () => answerIn(s.key),
      rename: () => answerIn(s.key, true),
      worktrees: board?.lanes,
      editors: board ? { apps: board.apps, icons } : undefined,
      desktop: onDesktop,
  });
  const sessionMenuSpec = (lane: Lane, s: Session): MenuSpec => menus.sessionMenu(lane, s, quickCan(lane, s));

  /** A PR's menu, as every window has it. */
  const prMenuSpec = (pr: PrItem): MenuSpec => {
    const s = prSession(pr);
    return menus.prMenu(pr, {
      report: menuReport,
      open: () => openPr(pr),
      session: s ? { ...s, open: () => openSession(s.lane, s.session) } : undefined,
      reveal: () => revealPr(pr),
      desktop: onDesktop,
    });
  };

  /** A row's menu: a session's, PR's or todo's as every window has it, a suggestion's own. */
  const rowMenu = (r: Row) => {
    if (r.kind === 'more' || r.kind === 'skill') return;
    const show = (spec: MenuSpec) => void showContextMenu(spec).catch((e: unknown) => setError(errorText(e)));
    if (r.kind === 'suggest') {
      const sg = r.s;
      if (sg.kind === 'send' || sg.kind === 'open') {
        show([...(sg.kind === 'send' ? [{ label: 'Send this prompt to it', run: () => send(sg.session) }, 'separator' as const] : []), ...sessionMenuSpec(sg.lane, sg.session)]);
      } else {
        show([
          { label: `Start a session in ${sg.lane.callsign}`, run: () => start({ lane: sg.lane, newWorktree: sg.kind === 'start' && sg.newWorktree, agent: sg.kind === 'start' ? sg.agent : undefined }) },
          { label: `Save as a todo in ${sg.lane.project}`, run: () => saveTodo(sg.lane) },
        ]);
      }
      return;
    }
    if (r.kind === 'session') {
      show(sessionMenuSpec(r.lane, r.session));
      return;
    }
    if (r.kind === 'pr' || r.kind === 'review') {
      show(prMenuSpec(r.pr));
      return;
    }
    const s = todoSession(r);
    show(
      menus.todoMenu(r.item.todo, {
        report: menuReport,
        start: () => prepareTodo(r),
        session: s ? { ...s, open: () => openSession(s.lane, s.session) } : undefined,
        reveal: () => revealTodo(r),
        // Straight to the engine, whose board shows it. Deleting is left to the main window,
        // which can put a todo back.
        edit: { run: (op) => void todoOp(op).catch((e: unknown) => menuReport.error(errorText(e))), todos: board?.todos, labels: board?.labels, projects: todoProjects(projects) },
        desktop: onDesktop,
      }),
    );
  };

  /* ---- Starting ---- */

  const ready = !!lane && !!account && !!placeNow;
  const terminalName = places.find((p) => p.place === 'terminal')?.label ?? 'The terminal';
  const placeShown = places.find((p) => p.place === placeNow);
  const onModel = onModelWords(startsOn, effortWord, models);
  const behind = places.find((p) => p.place === 'background')?.behind ?? false;
  const happens = startHappens({
    lane,
    account,
    place: placeNow,
    mac: deviceNow ? { name: deviceInfo?.name ?? null, error: remote?.error ?? null } : null,
    reading: !board || board.loading,
    newWorktree: inNewWorktree,
    onModel,
    prompted,
    terminal: terminalName,
    behind,
  });
  // Only the new-thread page adds the folder as a project; a thread Codealong starts on a prompt
  // shows under the app's Recents.
  const newToCodex = placeNow === 'codex-app' && !prompted && !!lane && !!startOptionsNow && !startOptionsNow.codexProjects.includes(lane.path);

  /** Keeps where a start on this Mac opened as its accounts' `start` in config.toml
   *  (`keepPlaceFor`), so the next start opens there, from here or from `codealong start`; and in
   *  the copy of where accounts can start, so the next open goes by it without reading again. */
  const keepPlace = (names: string[], p: StartPlace) => {
    if (names.length === 0) return;
    keepStartPlace(names, p)
      .then(() => {
        clearNotice(KEEP_PLACE_NOTICE);
        const cached = cachedOptions.current;
        if (cached) cachedOptions.current = { ...cached, options: withChosen(cached.options, names, p) };
        setOptions((o) => o && withChosen(o, names, p));
      })
      // The start went ahead; only where the next one opens wasn't saved, which the notice says.
      .catch((e: unknown) => reportNotice(keepPlaceFailed(e)));
  };

  /** Starts a session: in the worktree chosen below, or where a suggestion says. */
  const start = (over?: { lane: Lane; newWorktree: boolean; agent?: string }) => {
    const where = over?.lane ?? lane;
    const sgAgent = over?.agent ? resolveSuggestedAgent(over.agent, accounts, routingGroups, usage, now, allLanes) : null;
    const targetAccount = sgAgent?.account ?? account;
    const targetModel = sgAgent ? sgAgent.model : modelNow;
    if (!where || !targetAccount || !placeNow) return;
    // Improve Codealong's pictures are this Mac's files: a session on another Mac can't read them.
    const held = improve ? improveRemoteHold(improve.shots, shortOf(where.id) ? (deviceInfo?.name ?? 'another Mac') : null) : null;
    if (held) {
      setError(held);
      return;
    }
    const newTree = harnessMakesWorktree && !where.startsChats && (over ? over.newWorktree : newWorktree);
    if (choice) remember(choice, placeNow);
    if (!deviceNow) keepPlace(keepPlaceFor(choice, accounts, placeNow, routingGroups, optionsForStart?.autoGroup), placeNow);
    // The skill that starts the prompt goes as the account's agent takes it.
    const base = skillNow && harness && !over ? writtenFor(text, [skillNow.skill], harness) : text;
    const written = withAttachments(base, attached);
    // Improve Codealong: the pictures of the windows go after what you wrote.
    const prompt = withShots(written);
    const spec: StartSpec = { laneId: where.id, account: targetAccount, place: placeNow, prompt, model: targetModel, effort: effortNow, worktree: newTree, todos: forTodos.map((t) => t.id) };
    const targetOnModel = sgAgent ? onModelWords(sgAgent.model, effortWord, models) : onModel;
    const label = startingLabel(placeNow, targetAccount, targetOnModel, where, newTree, deviceInfo?.name ?? null);
    const id = Date.now();
    const to = opener ?? 'main';
    // The panel is fresh for next time; a failed start brings back what you had.
    const kept = { text, attached, todoIds, newWorktree, improve };
    keepPrompt(fullPrompt);
    clearPrompt();
    setTodoIds([]);
    setNewWorktree(false);
    setImprove(null);
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
          setAttached(kept.attached);
          setTodoIds(kept.todoIds);
          setNewWorktree(kept.newWorktree);
          setImprove(kept.improve);
          setError(errorText(e));
        });
      });
  };

  /** Sends what's typed to a session that's already going. The panel closes at once; a failed
   *  send brings it back with what you wrote and why. */
  const send = (session: Session) => {
    const words = textNow.current;
    const keptAttached = attachedNow.current;
    const full = withAttachments(words, keptAttached);
    // Improve Codealong's pictures go with what's sent too, as they go with a start.
    const prompt = withShots(full);
    const kept = improve;
    const id = Date.now();
    const to = opener ?? 'main';
    keepPrompt(full);
    clearPrompt();
    setImprove(null);
    setActiveKey(null);
    setError(null);
    void tellOpener(to, { phase: 'begin', id, label: `Sending to ${session.title}…` });
    void hideQuick();
    sendAhead(session, prompt, { toDraft: false })
      .then((sent) => tellOpener(to, { phase: 'end', id, key: null, message: sent.message }))
      .catch((e: unknown) => {
        void tellOpener(to, { phase: 'end', id, key: null, message: null });
        reopen(() => {
          setText(words);
          setAttached(keptAttached);
          setImprove(kept);
          setError(errorText(e));
        });
      });
  };

  /** Keeps what's typed as a todo, linked to the worktree. Improve Codealong's pictures go into
   *  its description. */
  const saveTodo = (where: Lane) => {
    const words = textNow.current.trim();
    const keptAttached = attachedNow.current;
    const full = withAttachments(words, keptAttached);
    const prompt = withShots(full);
    const title = todoTitle(words || full);
    const t = newTodo({ title, description: prompt === title ? '' : prompt, project: where.project, worktrees: [where.id] }, Date.now());
    const to = opener ?? 'main';
    const kept = improve;
    keepPrompt(full);
    clearPrompt();
    setImprove(null);
    setActiveKey(null);
    setError(null);
    void hideQuick();
    todoOp({ op: 'create', todo: t })
      .then(() => tellOpener(to, { phase: 'end', id: Date.now(), key: null, message: `Saved “${title}” as a todo in ${where.project}.` }))
      .catch((e: unknown) =>
        reopen(() => {
          setText(words);
          setAttached(keptAttached);
          setImprove(kept);
          setError(errorText(e));
        }),
      );
  };

  const suggest = (sg: Suggestion, alt: boolean) => {
    if (sg.kind === 'send') alt ? revealSession(sg.lane, sg.session) : send(sg.session);
    else if (sg.kind === 'open') (alt ? openSession : showSession)(sg.lane, sg.session);
    else if (sg.kind === 'start') start({ lane: sg.lane, newWorktree: sg.newWorktree, agent: sg.agent });
    else saveTodo(sg.lane);
  };

  /** Finishes the name as the row's skill, in its repo: what follows is the prompt, and ⏎ then
   *  starts the session. */
  const pickSkill = (r: SkillRow) => {
    const at = typing ?? mentionsIn(textNow.current).find((m) => m.name === r.skill.name);
    if (at) {
      const next = completed(textNow.current, at, r.skill);
      setText(next.text);
      setCaret(next.caret);
      window.requestAnimationFrame(() => {
        input.current?.focus();
        input.current?.setSelectionRange(next.caret, next.caret);
        // Focusing puts the old selection back for a moment, which the box reports.
        setCaret(next.caret);
      });
    }
    setSkillKey(r.key);
    setActiveKey(COMPOSE);
    setError(null);
  };

  const activate = (key: string, alt: boolean) => {
    const row = rows.find((r) => r.key === key);
    if (!row) start();
    else if (row.kind === 'suggest') suggest(row.s, alt);
    else if (row.kind === 'todo') (alt ? revealTodo : openTodo)(row);
    else if (row.kind === 'pr') (alt ? openPr : showPr)(row.pr);
    else if (row.kind === 'review') (alt ? revealPr(row.pr) : answerIn(row.pr.key));
    else if (row.kind === 'skill') pickSkill(row);
    else if (row.kind === 'more') {
      // The first of those it adds takes its place.
      setAllFor(query);
      setActiveKey(ranked[FOUND_SESSIONS]?.key ?? null);
    } else (alt ? openSession : showSession)(row.lane, row.session);
  };

  /** A click on a row (or the new-session row) does it when the pointer was moved onto it, and
   *  otherwise only selects it: the list may have moved it under a pointer that stayed put, as
   *  Jev's suggestions and what sessions said come in (aim.ts). */
  const clickRow = (key: string, e: React.MouseEvent) => {
    if (clickDoes(aim.current, key)) return activate(key, e.altKey);
    aim.current = aimedAt(key, e.screenX, e.screenY);
    setActiveKey(key);
  };

  /** The pointer moving over the list: the row it's moved onto is the one aimed at, and hovering
   *  selects it. A row that comes under a pointer that stays put is neither. */
  const pointerAt = (e: React.MouseEvent) => {
    const key = (e.target as Element).closest('[data-row]')?.getAttribute('data-row') ?? null;
    const next = aimAfter(aim.current, key, e.screenX, e.screenY);
    aim.current = next.aim;
    if (next.moved && key && key !== active && keys.includes(key)) setActiveKey(key);
  };

  /* ---- Keys ---- */

  const togglePicker = (k: PickerKind) => setPicker((p) => (p === k ? null : k));

  const onInputKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    const el = e.currentTarget;
    // ⇥ on a session answers it in the waiting window; elsewhere it moves the focus as usual.
    if (e.key === 'Tab' && !e.shiftKey && !e.metaKey && !e.altKey && !e.ctrlKey) {
      const row = rows.find((r) => r.key === active);
      if (row?.kind === 'skill') {
        e.preventDefault();
        pickSkill(row);
        return;
      }
      const session = row?.kind === 'session' ? row.session : row?.kind === 'suggest' && (row.s.kind === 'send' || row.s.kind === 'open') ? row.s.session : null;
      if (session) {
        e.preventDefault();
        answerIn(session.key);
        return;
      }
      // And on a PR waiting on your review, it reviews it there, as ⏎ does.
      if (row?.kind === 'review') {
        e.preventDefault();
        answerIn(row.pr.key);
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
    const recalled = recall !== null && withAttachments(el.value, attached) === recallable[recall];
    const plainKey = !e.altKey && !e.metaKey && !e.shiftKey && !e.ctrlKey;
    const walk = (e.key === 'ArrowUp' && (recalled || (!el.value && attached.length === 0 && keys.indexOf(active) <= 0))) || (e.key === 'ArrowDown' && recalled);
    if (plainKey && walk) {
      const at = recalled ? recall + (e.key === 'ArrowUp' ? 1 : -1) : 0;
      if (at >= recallable.length) return e.preventDefault();
      e.preventDefault();
      const next = at < 0 ? '' : recallable[at];
      const shownText = loadPrompt(next);
      setRecall(at < 0 ? null : at);
      setTodoIds(at === 0 && draft && recallable[0] === draft.text ? draft.todos : []);
      setActiveKey(null);
      window.requestAnimationFrame(() => el.setSelectionRange(shownText.length, shownText.length));
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
        } else void hideQuick();
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
      // ⇧⌘A, while there's a search: the box on Sessions' heading. ⌘A alone selects what's typed.
      if (k === 'a' && e.shiftKey && query) {
        e.preventDefault();
        toggleArchived();
        return;
      }
      if (k === 'p' || k === 't' || k === 'l' || (k === 'd' && macs.length > 0) || (k === 'o' && places.length > 1)) {
        e.preventDefault();
        togglePicker(k === 'p' ? 'project' : k === 'd' ? 'device' : k === 't' ? 'worktree' : k === 'o' ? 'place' : 'agent');
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

  const worktreeSections: PickerSection[] = [
    // First, so a long list of worktrees doesn't hide it.
    ...(chatLane ? [{ key: 'chat', options: [{ id: chatLane.id, text: 'no project chat question', label: NO_PROJECT, detail: 'A chat of its own', lead: <Icon name="message-circle" size={12} /> }] }] : []),
    ...groups.map((g) => ({
      key: g.label,
      head: g.label,
      options: g.lanes.map((l) => {
        const branch = laneBranch(l);
        return {
          id: l.id,
          text: [l.callsign, l.branch ?? '', l.repo, l.project, l.origin === 'main' ? 'main checkout' : ''].join(' '),
          label: l.origin === 'main' ? `${l.callsign} (main checkout)` : l.callsign,
          detail: branch,
          lead: <StatusGlyph status={l.glyph} size={10} title="" waiting={laneWaits(l)} />,
        };
      }),
    })),
  ];

  const agentSections: PickerSection[] = useMemo(
    () => agentMenuSections(agents, accounts, pick, usage, models, now, agentNow, AUTO_STARTS, routingGroups, allLanes),
    [agents, accounts, pick, usage, models, now, agentNow, routingGroups, allLanes],
  );

  const pickAgent = (id: string) => {
    pickAgentLine(id);
    setPicker(null);
    refocus();
  };

  const effortFoot = efforts.length > 0 ? <EffortFoot efforts={efforts} value={effort} onPick={setEffort} harness={harness} model={effortModel} /> : null;

  const deviceSections: PickerSection[] = useMemo(() => {
    if (!devices) return [];
    const count = (short: string | null) => worktreeCount((short ? devices.lanes.filter((l) => shortOf(l.id) === short) : (board?.lanes ?? [])).filter((l) => !l.startsChats && inProject(l, current)).length, current);
    const here = devices.me.name || 'This Mac';
    return [
      {
        key: 'macs',
        options: [
          { id: HERE, text: `${here} this mac`, label: here, detail: `This Mac · ${count(null)}`, lead: <Icon name="device-desktop" size={14} /> },
          ...macs.map((d) => {
            const { away, off, title } = macUnavailable(d, now);
            return {
              id: d.short,
              text: d.name,
              label: d.name,
              detail: away ?? off ?? count(d.short),
              lead: <i className={`ca-labeldot is-${d.color}`} aria-hidden="true" />,
              disabled: !!(away || off),
              title,
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

  /** A suggestion's row: what ⏎ does, and where. */
  const describe = (sg: Suggestion) => {
    const sgAgent = sg.kind === 'start' && sg.agent ? resolveSuggestedAgent(sg.agent, accounts, routingGroups, usage, now, allLanes) : null;
    const sgAccount = sgAgent?.account ?? account;
    const sgOnModel = sgAgent ? onModelWords(sgAgent.model, effortWord, models) : onModel;
    return suggestionWords(sg, { now, text, account: sgAccount, onModel: sgOnModel, place: placeNow, makesWorktree, terminal: terminalName, behind, duplicate });
  };

  /** What a suggestion's row shows, but its keys. */
  const suggestionFace = (sg: Suggestion) => {
    const said = describe(sg);
    return (
      <>
        <span className="ca-quick-plus is-suggest">
          <Icon name={said.icon} size={13} stroke={2} />
        </span>
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">{said.title}</span>
          <span className="ca-quick-row-why">{said.why}</span>
        </span>
        <span className="ca-quick-row-where">{said.where}</span>
        <span className={`ca-quick-sure is-${sureness(sg.p)}`}>{Math.round(sg.p * 100)}%</span>
      </>
    );
  };

  /** What Suggested shows while it has no suggestion for what's typed: the last answer's, dimmed
   *  and doing nothing, while Jev works out the next; else a line in a suggestion's shape, so the
   *  rows under it don't move when suggestions come. */
  const suggestedStandIn =
    lastSuggested.length > 0 ? (
      lastSuggested.map((r) => (
        <div key={r.key} role="option" aria-selected={false} aria-disabled="true" data-row={r.key} className="ca-quick-row is-suggest is-stale" onMouseDown={(e) => e.preventDefault()}>
          {suggestionFace(r.s)}
          {/* No key while it does nothing; the column stays, so nothing beside it moves. */}
          <span className="ca-kbds" aria-hidden="true" />
        </div>
      ))
    ) : (
      <div className={cx('ca-quick-row', 'is-suggest', 'is-note', routing && 'is-routing')}>
        <span className="ca-quick-plus is-suggest">
          <Icon name={routeProblem ? 'alert' : 'sparkles'} size={13} stroke={2} />
        </span>
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">{routing ? 'Working out where this goes…' : routeProblem ? 'Couldn’t suggest where this goes' : 'No suggestion for this'}</span>
          <span className="ca-quick-row-why">{routing ? 'Jev is reading what you’ve typed.' : (routeProblem ?? 'Start it below, or keep typing.')}</span>
        </span>
      </div>
    );

  /* ---- Render ---- */

  const sections: { group: RowGroup; rows: Row[] }[] = [];
  for (const r of rows) {
    const last = sections[sections.length - 1];
    if (last && last.group === r.group) last.rows.push(r);
    else sections.push({ group: r.group, rows: [r] });
  }
  // While what's typed goes to Jev, Suggested is there before Jev answers and when it has nothing
  // to suggest, so the rows under it don't move as answers come (suggestedStandIn, below).
  if (routeFor && sections[0]?.group !== 'suggested') sections.unshift({ group: 'suggested', rows: [] });
  // While there's a search, Sessions' heading is there even with no session found: it has the box
  // that lists archived sessions too.
  if (query && !sections.some((x) => x.group === 'found')) sections.splice(sections[0]?.group === 'suggested' ? 1 : 0, 0, { group: 'found', rows: [] });
  // Whether a search found nothing, Jev's suggestions aside.
  const foundNothing = !!query && rows.every((r) => r.kind === 'suggest');
  const nothingWaits = listing && !!attention && attention.counts.needs + attention.counts.reviews + attention.counts.unread === 0;
  /** A group's heading count: all there are of it, when the list shows only some. */
  const groupCount = (g: RowGroup, shown: number): number => {
    if (g === 'ready') return readyList?.count ?? shown;
    if (g === 'recent') return recent?.count ?? shown;
    if (g === 'needs' || g === 'reviews' || g === 'unread' || g === 'working') return attention?.counts[g] ?? shown;
    if (g === 'found') return foundSessions.length;
    return shown;
  };
  // Another Mac's accounts come a moment after it's picked, or not at all.
  const activeGroup = isGroup(choice, routingGroups);
  const agentChip = agentChipWords({ auto: isAuto(choice), group: activeGroup?.name ?? null, account, accounts: accounts.length, remote: deviceNow ? { read: !!startOptionsNow, error: remote?.error ?? null } : null, model: modelShown, effortWord });
  // The hint names the shortcut that opened the panel this way: Improve Codealong's when improving,
  // the chat one when it opened on a chat.
  const shortcut = shortcutLabel(improve ? board?.improveShortcut : asChat ? board?.chatShortcut : board?.quickShortcut);

  // PR links in the list's rows (PrLink.tsx): a PR, or a session on its card, shows in the main
  // window, as ⏎ on a row does.
  usePrLinks(board?.prs, board?.lanes, {
    showPr: revealPr,
    showSession,
    openSession,
    sessionMenu: (lane, s) => void showContextMenu(sessionMenuSpec(lane, s)).catch((e: unknown) => setError(errorText(e))),
    prMenu: (url, pr) => {
      const spec = pr ? prMenuSpec(pr) : menus.prMenu({ url, number: prRefOf(url)?.number ?? 0 }, { report: menuReport, open: () => openLink(url) });
      void showContextMenu(spec).catch((e: unknown) => setError(errorText(e)));
    },
    openLink,
  }, board);

  return (
    <div
      className="ca-quick-root"
      onMouseDown={(e) => {
        // The shadow around the card is part of the window; a click there is a click outside.
        if (e.target === e.currentTarget) void hideQuick();
      }}
    >
      {drag.ghost}
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
            placeholder={improve ? IMPROVE_PLACEHOLDER : draft ? 'Find a session, or describe new work… ↑ brings back your draft' : history.length ? 'Find a session, or describe new work… ↑ for earlier prompts' : 'Find a session, or describe new work…'}
            onChange={(e) => {
              setText(e.target.value);
              setCaret(e.target.selectionStart);
              setRecall(null);
              setActiveKey(null);
            }}
            onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
            onKeyDown={onInputKey}
            onPaste={(e) => {
              const files = filesToAttach(e);
              if (files.length === 0) return;
              e.preventDefault();
              keepFiles(files)
                .then(addAttachments)
                .catch((err: unknown) => setError(errorText(err)));
            }}
          />
          {shortcut && !text && (
            <span className="ca-quick-hotkey" title="Opens this panel from any app. Change it in Settings.">
              {shortcut}
            </span>
          )}
        </div>
        {attached.length + shots.length > 0 && (
          <div className="ca-quick-attached">
            <AttachmentStrip
              items={[...shots, ...attached]}
              onRemove={(a) => (shots.some((s) => s.path === a.path) ? setImprove((i) => i && { shots: i.shots.filter((s) => s.path !== a.path) }) : setAttached((prev) => prev.filter((x) => x.path !== a.path)))}
            />
          </div>
        )}
        </div>
        <div
          ref={list}
          className="ca-quick-list"
          role="listbox"
          aria-label="What to do next"
          onMouseMove={pointerAt}
          onWheel={() => {
            aim.current = aimedByScroll(aim.current);
          }}
          onMouseLeave={() => {
            aim.current = aimedAtNothing(aim.current);
          }}
        >
          <div ref={scroller} className="ca-quick-scroll">
            {!board || board.loading ? (
              <>
                <p className="ca-quick-note">{error ? '' : 'Reading sessions…'}</p>
                {!error && <Tip place="quick" />}
              </>
            ) : (
              <>
                {nothingWaits && (
                  <p className="ca-quick-note">
                    Nothing needs you{attention.counts.working > 0 ? ` · ${attention.counts.working} working` : ''}.
                  </p>
                )}
                {typing && rows.length === 0 && <p className="ca-quick-note">{listedSkills ? (repos.length === 0 ? 'No skill of your accounts starts like that, and no repo here to read others from.' : `No skill of ${current === ALL_PROJECTS ? 'these repos' : `${current}’s repos`} or your accounts starts like that.`) : 'Reading skills…'}</p>}
                {sections.map((s) => (
                  <div key={s.group} ref={s.group === 'suggested' ? suggestedGroup : undefined} className="ca-quick-group" role="group" aria-label={GROUP_WORD[s.group]}>
                    <div className={`ca-quick-glabel is-${s.group}`}>
                      {GROUP_WORD[s.group]}
                      {s.group !== 'suggested' ? (
                        <span>{groupCount(s.group, s.rows.length)}</span>
                      ) : s.rows.length > 0 ? (
                        <span>{s.rows.length}</span>
                      ) : (
                        routing &&
                        lastSuggested.length > 0 && (
                          <span className="ca-quick-glabel-routing" title="Working out where this goes…">
                            <Icon name="sparkles" size={11} />
                          </span>
                        )
                      )}
                      {s.group === 'found' && (
                        <label
                          className="ca-check ca-quick-archived"
                          title={withArchived ? 'Searches list archived sessions after the others. ⇧⌘A leaves them out' : `Searches leave archived sessions out${archivedFound > 0 ? `; ${archivedFound} ${archivedFound === 1 ? 'matches' : 'match'} this one` : ''}. ⇧⌘A lists them`}
                          // The keyboard stays in the box.
                          onMouseDown={(e) => e.preventDefault()}
                        >
                          <input
                            type="checkbox"
                            checked={withArchived}
                            onChange={() => {
                              toggleArchived();
                              refocus();
                            }}
                          />
                          <span className="ca-check-box" aria-hidden="true">
                            <Icon name="check" size={11} stroke={2.5} />
                          </span>
                          Archived
                          {archivedFound > 0 && <span className="ca-quick-archived-count">{archivedFound}</span>}
                        </label>
                      )}
                    </div>
                    {s.rows.map((r) => {
                      const n = rows.indexOf(r) + 1;
                      // What every row does with the pointer: hovering selects (the list's
                      // pointerAt, by the row's data-row), a click does it, a right-click has the
                      // rest, and dragging it out of the panel pins it to the desktop.
                      const held = drag.rowProps(r.key, null, 0);
                      const handlers = {
                        'data-row': r.key,
                        onPointerDown: held.onPointerDown,
                        onMouseDown: (e: React.MouseEvent) => e.preventDefault(),
                        onClick: (e: React.MouseEvent) => clickRow(r.key, e),
                        onContextMenu: (e: React.MouseEvent) => {
                          e.preventDefault();
                          rowMenu(r);
                        },
                      };
                      if (r.kind === 'suggest') {
                        const said = describe(r.s);
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={cx('ca-quick-row', 'is-suggest', active === r.key && 'is-active', held.extraClass)}
                            title={`${said.title}\n${said.why}\n${Math.round(r.s.p * 100)}% likely, from ${route?.model ?? 'Jev'} in ${route?.ms ?? 0} ms · ⏎ does it${r.s.kind === 'send' ? ' · ⌥⏎ shows the session in Codealong' : r.s.kind === 'open' ? ' · ⌥⏎ opens the session where it lives' : ''}`}
                            {...handlers}
                          >
                            {suggestionFace(r.s)}
                            {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                          </div>
                        );
                      }
                      if (r.kind === 'skill') {
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={cx('ca-quick-row', 'is-skill', active === r.key && 'is-active')}
                            title={`/${r.skill.name}${r.skill.hint ? ` ${r.skill.hint}` : ''}\n${r.skill.description}\n${r.lane ? `In ${r.lane.repo}` : `${r.accounts.join(', ')}’s own`}${r.skill.uses > 0 ? ` · run ${r.skill.uses} ${r.skill.uses === 1 ? 'time' : 'times'}` : ''}\n⏎ or ⇥ finishes the name; ${r.lane ? `a session on it starts in ${r.lane.repo}` : 'a session on it starts where it’s set to below'}`}
                            data-row={r.key}
                            onMouseDown={handlers.onMouseDown}
                            onClick={handlers.onClick}
                          >
                            <span className="ca-quick-plus is-suggest">
                              <Icon name="slash" size={13} stroke={2} />
                            </span>
                            <span className="ca-quick-row-main">
                              <span className="ca-quick-row-title">
                                <Marked text={`/${r.skill.name}`} marks={r.marks} />
                                {r.skill.hint && <span className="ca-quick-row-hint"> {r.skill.hint}</span>}
                              </span>
                              <span className="ca-quick-row-why">{r.skill.description}</span>
                            </span>
                            <span className="ca-quick-row-where">{skillWhere(r)}</span>
                            {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                          </div>
                        );
                      }
                      if (r.kind === 'todo') {
                        const t = r.item.todo;
                        const opens = todoSession(r) ? 'shows its session in Codealong' : 'gets a session ready to start for it';
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={cx('ca-quick-row', 'is-todo', active === r.key && 'is-active', held.extraClass)}
                            title={`${t.title || 'Untitled'}\n⏎ ${opens} · ⌥⏎ shows the todo in Codealong`}
                            {...handlers}
                          >
                            <TodoGlyph status={t.status} waiting={r.waiting} />
                            <span className="ca-quick-row-main">
                              <span className="ca-quick-row-title">
                                <Marked text={t.title || 'Untitled'} marks={r.marks} />
                              </span>
                              <span className="ca-quick-row-why">{todoWhy(r, now)}</span>
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
                            className={cx('ca-quick-row', 'is-more', active === r.key && 'is-active', held.extraClass)}
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
                      if (r.kind === 'pr' || r.kind === 'review') {
                        const pr = r.pr;
                        return (
                          <div
                            key={r.key}
                            role="option"
                            aria-selected={active === r.key}
                            className={cx('ca-quick-row', 'is-pr', active === r.key && 'is-active', held.extraClass)}
                            title={`${pr.title}\n${pr.repoSlug}#${pr.number} · ${pr.head}\n${r.kind === 'review' ? '⏎ reviews it here · ⌥⏎ shows it in Codealong' : '⏎ shows it in Codealong · ⌥⏎ opens it on GitHub'}`}
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
                          className={cx('ca-quick-row', active === r.key && 'is-active', held.extraClass, `is-${s2.status}`)}
                          title={`${s2.title}\n${s2.account} · ${r.lane.callsign}\n⏎ shows it in Codealong · ⇥ answers it in Waiting on you · ⌥⏎ opens it where it lives`}
                          {...handlers}
                        >
                          <SessionGlyph s={s2} />
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
                          {s2.archived && (
                            <Chip title="Archived: listed because searches list archived sessions too">
                              <Icon name="archive" size={11} />
                              Archived
                            </Chip>
                          )}
                          <span className="ca-quick-row-where">
                            {s2.account} · {r.lane.callsign}
                          </span>
                          <span className="ca-quick-row-age">{ago(s2.statusSince ?? s2.lastActivity, now)}</span>
                          {active === r.key && <span className="ca-quick-row-tab">⇥ Reply</span>}
                          {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
                        </div>
                      );
                    })}
                    {s.group === 'suggested' && s.rows.length === 0 && suggestedStandIn}
                    {/* Under Sessions' heading rather than over the list, so they don't move
                        Suggested as they come and go. */}
                    {s.group === 'found' && foundNothing && (
                      <p className="ca-quick-note">
                        {archivedFound > 0 && !withArchived
                          ? `No sessions, todos or PRs match, but ${archivedFound} archived ${archivedFound === 1 ? 'session does' : 'sessions do'}: ⇧⌘A lists ${archivedFound === 1 ? 'it' : 'them'}. Or start a session with this prompt below.`
                          : 'No sessions, todos or PRs match. Start a session with this prompt below.'}
                      </p>
                    )}
                    {s.group === 'found' && textFound?.building && <p className="ca-quick-note">Still reading what recent sessions said, so more may match in a moment.</p>}
                  </div>
                ))}
              </>
            )}
          </div>
          <div
            role="option"
            aria-selected={active === COMPOSE}
            className={cx('ca-quick-row', 'ca-quick-compose', active === COMPOSE && 'is-active', !ready && 'is-disabled')}
            data-row={COMPOSE}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => clickRow(COMPOSE, e)}
          >
            <span className="ca-quick-plus">
              <Icon name="plus" size={14} stroke={2} />
            </span>
            <span className="ca-quick-row-main">
              <span className="ca-quick-row-title">{startTitle(forTodos.length, lane ? startTitleWords(lane, inNewWorktree) : null)}</span>
              <span className="ca-quick-row-why">
                {/* The warning leads: the row's line is cut short when it's long. */}
                {primaryRunningWords(held, forTodos.length)}
                {skillNow && (skillNow.lane ? `Runs /${skillNow.skill.name} from ${skillNow.lane.repo}. ` : `Runs /${skillNow.skill.name}. `)}
                {happens}
                {skillLeftOut && ` ${skillLeftOut}`}
                {alreadyOn}
                {attached.length + shots.length > 0 && attachmentsNote(attached.length + shots.length)}
                {newToCodex && ' Codex adds this folder as a project if it isn’t one yet.'}
                {deviceNow && forTodos.length > 0 && todosStayNote(forTodos.length)}
              </span>
            </span>
            {lane && <span className="ca-quick-row-where">{startWhereWords(lane)}</span>}
            {/* Where a suggestion says how likely it is, so the columns line up with theirs. */}
            <span className="ca-quick-sure" aria-hidden="true" />
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
            {held.length > 0 && (
              <button
                type="button"
                className="ca-quick-todo-wait ca-quick-todo-primary"
                title={held.map(({ todo: t, at }) => `“${at.session.title}” (${at.session.account} · ${at.lane.callsign}) is ${forTodos.length > 1 ? `“${t.title || 'Untitled'}”’s` : 'its'} primary session. Click to show it.`).join('\n')}
                onClick={() => showSession(held[0].at.lane, held[0].at.session)}
              >
                <Icon name="user" size={12} />
                <span className="ca-quick-todo-primary-text">{held.length === 1 ? `“${held[0].at.session.title}” is on it` : `${held.length} already have a session`}</span>
              </button>
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
        {info && !error && (
          <p className="ca-quick-error is-info" role="status">
            <Icon name="check" size={14} />
            <span>{info}</span>
          </p>
        )}
        {improveNote && !error && !info && (
          <p className="ca-quick-error is-info ca-quick-improve" role="status">
            <Icon name="bulb" size={14} />
            <span>{improveNote.text}</span>
            {improveNote.add && (
              <Button size="sm" onMouseDown={(e) => e.preventDefault()} onClick={addCodealongProject}>
                Add a {CODEALONG_PROJECT} project
              </Button>
            )}
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
            <button type="button" data-picker-trigger className="ca-quick-chip" title={lane ? `${lane.startsChats ? 'A chat of its own, in no project' : tildePath(lane.path)}\nWorktree (⌘T)` : 'Worktree (⌘T)'} disabled={lanes.length === 0 && !chatLane} aria-haspopup="dialog" aria-expanded={picker === 'worktree'} onClick={() => togglePicker('worktree')}>
              <Icon name={lane?.startsChats ? 'message-circle' : 'folder'} size={14} />
              <span>{lane ? (lane.startsChats ? NO_PROJECT : lane.origin === 'main' ? `${lane.callsign} (main)` : lane.callsign) : 'No worktree'}</span>
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'worktree' && <Picker sections={worktreeSections} value={lane?.id ?? null} placeholder="Worktree, branch or repo" onPick={(id) => { setLaneId(id); setPicker(null); refocus(); }} onClose={() => { setPicker(null); refocus(); }} />}
          </div>
          <label
            className={cx('ca-check', 'ca-quick-newtree', !makesWorktree && 'is-disabled')}
            title={lane?.startsChats ? 'A chat has no repo to make a worktree of.' : newWorktreeTitle(makesWorktree, placeNow, harness)}
          >
            <input type="checkbox" checked={inNewWorktree} disabled={!makesWorktree} onChange={(e) => setNewWorktree(e.target.checked)} />
            <span className="ca-check-box" aria-hidden="true">
              <Icon name="check" size={11} stroke={2.5} />
            </span>
            New worktree
          </label>
          {places.length > 1 && (
            <div className="ca-quick-pick ca-quick-place">
              <button type="button" data-picker-trigger className="ca-quick-chip" title="Opens in (⌘O)" aria-haspopup="dialog" aria-expanded={picker === 'place'} onClick={() => togglePicker('place')}>
                {placeShown && <Icon name={placeIcon(placeShown.place)} size={14} />}
                <span>{placeShown?.label ?? 'Opens in'}</span>
                <Icon name="chevron-down" size={12} />
              </button>
              {picker === 'place' && <Picker sections={placeSections(places)} value={placeNow} placeholder="Opens in" onPick={(id) => { pickPlace(id as StartPlace); setPicker(null); refocus(); }} onClose={() => { setPicker(null); refocus(); }} />}
            </div>
          )}
          <div className="ca-quick-pick ca-quick-agent">
            <button type="button" data-picker-trigger className="ca-quick-chip" title="Account, model and effort (⌘L)" disabled={accounts.length === 0} aria-haspopup="dialog" aria-expanded={picker === 'agent'} onClick={() => togglePicker('agent')}>
              <Icon name={autoFor ? 'sparkles' : 'user'} size={14} />
              <span>{agentChip.label}</span>
              {agentChip.detail && <span className="ca-quick-chip-detail">{agentChip.detail}</span>}
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'agent' && <Picker className="is-wide" sections={agentSections} value={agentNow} placeholder="Account or model" onPick={pickAgent} onClose={() => { setPicker(null); refocus(); }} foot={effortFoot} onStep={effortFoot ? stepEffort : undefined} />}
          </div>
        </div>
      </div>
    </div>
  );
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
