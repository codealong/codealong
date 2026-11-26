// The quick panel's page (quick.rs has its window): one input, and below it what you can do
// next. With nothing typed it lists the sessions that need you, then the ones whose turn ended,
// then a few todos ready to start, then the other sessions started in the last day; typed words find sessions, todos and PRs; the last row starts a
// new session with what you typed as its prompt, in the repo and the worktree in it, on the agent
// and in the app chosen at the bottom.
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
// to one (⌥⌘0 to all of them), ⌘D picks the Mac (with others linked), ⌘T the repo, ⌘B the
// worktree in it (a new one, one ready for new work, the main checkout, or one with work), ⌘L
// the agent, whose menu has the effort under its models (← → there), and ⌘O where it opens
// (with more than one place), ↑ in the empty input brings back the last draft and earlier
// prompts, and Escape closes. (⌘M is the Window menu's Minimize, which may take the key before
// the page sees it.)
//
// ⇥ on a session answers it, and ⏎ or ⇥ on a PR waiting on your review reviews it, in the waiting
// window (Waiting.tsx), which goes on from there to the rest that waits on you: the panel finds
// and starts, and answering is that window's.
//
// With voice on, the box takes what you say too (src-tauri/src/dictation.rs): the key held to
// talk opens the panel from another app as it goes down, and the microphone beside the box does
// the same with a click. The words show in the box as they're heard, selected, where they'll be
// typed once they're settled, and with Shift down as the key comes up they start a session at
// once, as ⌘⏎ would. What the box holds isn't touched until then: the words heard are drawn
// into it, not written to it.
//
// Opened to listen (`[shortcuts] listen`, Listen now in the windows) it's set to that instead:
// the box asks what the meeting is, nothing is listed or looked for, and ⏎ has Codealong hear it
// from then on, with a session started for it (meetings.rs `now`) in a chat of its own unless a
// repo or worktree is picked (⌘T, ⌘B). What it does with what it hears, and what's heard, are picked under
// the box as a session's Listen… asks them (Listening.tsx `ListenChoices`), and kept for next time.

import { reuse } from '../reuse';
import { REUSE_STATE } from '../reusable';
import type { PrTab } from '../agentReview';
import { aimAfter, aimAfterChange, aimedAt, aimedAtNothing, aimedByScroll, clickDoes, type Aim } from '../aim';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { flushSync } from 'react-dom';
import {
  addFolder,
  boardSeen,
  getBoard,
  getDevices,
  hideQuick,
  inPreview,
  getConfig,
  keepStartPlace,
  hearingStatus,
  listenNow,
  meetingsAhead,
  noteRoute,
  onBoard,
  onDevices,
  onQuickChat,
  onQuickHidden,
  onTalk,
  onTalked,
  openUrl,
  routeQuick,
  saveConfig,
  showMain,
  showQuick,
  showWaiting,
  startFromQuick,
  startOptions,
  talkTo,
  tellOpener,
  todoOp,
  type QuickOpen,
} from '../api';
import { CODEALONG_PROJECT, IMPROVE_PLACEHOLDER, improvePrompt, improveWords, remoteImproveLane, withCodealongProject } from '../improve';
import { devSetupTask, type AgentTask } from '../agentTasks';
import { askProblem, LISTEN_KEPT, listenKept, macSays, nowAsk, nowMeeting, nowPlaceholder, nowRow } from '../listening';
import { saidAfter } from '../drafts';
import { place } from '../talk';
import { boardAhead, sendAhead, useAhead } from '../ahead';
import { filesToAttach, keepFiles, useFileDrop } from '../attachInput';
import { deviceLabel, floatingLanes, shortOf } from '../devices';
import { calmerMachine, type Calmer } from '../machineLoad';
import { useDeviceLanes, useObserve } from '../useDevices';
import { usePerf } from '../perf';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { useEditorIcons } from '../editors';
import * as menus from '../menus';
import { prRefOf } from '../prLinks';
import { tildePath } from '../format';
import { useLogo } from '../logo';
import { showsStart } from '../newSession';
import { useRowDrag } from './useRowDrag';
import { ALL_PROJECTS, CHATS, GENERAL_CHAT, attachmentsNote, attentionRows, draftCodec, findPrs, foldedWords, GROUP_WORD, holdsPrompt as holdsAPrompt, findSessions, findTodos, folderChoices, folderStart, FOUND_SESSIONS, inProject, MAX_ROWS, nextDraft, primaryRunningWords, projectCounts, projectLabel, projectShownIn, QUICK_PLACEHOLDER, queryWords, readyTodos, recallList, pickInProject, recentRows, repoChoices, repoKey, repoWorktrees, SEND_TO_PLACEHOLDER, sendTargets, shortcutLabel, startLaneIn, startTitle, todosStayNote, worktreeName, type Machines, type QuickDraft, type RowGroup, type TodoRow, withPrompt as historyWith } from '../quick';
import {
  agentChipWords,
  autoGroupIn,
  autoPick,
  isAuto,
  isGroup,
  keepPlaceFailed,
  keepPlaceFor,
  newWorktreeDetail,
  newWorktreeTitle,
  onModelWords,
  ownModels,
  keptAt,
  placeStep,
  START_OPTIONS_NOTICE,
  startDoes,
  startHappens,
  startingLabel,
  startsOnFirstSend,
  startChoice,
  startOptionsFailed,
  startTitleWords,
  startWhereWords,
  withChosen,
} from '../start';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { completed, mentionAt, mentionsIn, namedBy, repoLanes, skillLeftOutWords, skillOffers, skillRows, skillStart, writtenFor, type Mention, type SkillRow } from '../skills';
import { useSkills } from '../useSkills';
import { alreadyOnWords, duplicateOf, rankedBy, routeChoice, routeDone, sameStart, startAgentOf, suggestedChoice, suggestionWords, todoLaneFor, todoTitle, toSessionSuggestion, worthRouting, type StartFacts, type Suggestion } from '../route';
import { liveOrLatest, sessionHomes } from '../lanes';
import { blockedBy, mergeAttachments, newTodo, primaryHolds, splitAttachments, todoAgent, todoItem, todoProjects, todoPrompt, todosPrompt, withAttachments } from '../todos';
import type { Attachment, Board, DevicesView, HearingStatus, Lane, ListenAsk, MeetingAhead, PrItem, Route, RouteChoice, RouteNote, Session, StartOptions, StartPlace, StartSpec, Talk } from '../types';
import { AttachmentStrip } from './DescriptionEditor';
import { DevSetupDialog } from './DevSetupDialog';
import { useDoneHelpers } from './DoneHelpers';
import { Level } from './Said';
import { filterWords, matches, Picker, type PickerSection } from './Picker';
import { addsTo, anotherFolderOption, chatOption, folderOptions, HERE, machineMenu, namedWorktreeOption, NEW_WORKTREE, newWorktreeOption, projectMenu, REPO, repoOption, repoSections, worktreeMenu } from './pickerOptions';
import { AgentFoot } from './AgentMenu';
import { agentMenuSections, placeIcon } from './StartControls';
import { Button, cx, Icon, Kbd, Mark } from './primitives';
import { ProjectIcon, ProjectIcons, projectIconMap, ProjectShown } from './ProjectSaid';
import { usePrLinks } from './PrLink';
import { useTheme } from './ThemeSwitch';
import { useGlass } from '../glass';
import { Tip } from './Tip';
import { useNow } from './useNow';
import { useShowStarted } from './useShowStarted';
import { errorText, type Report } from '../report';
import { openWhere, where } from '../open';
import { asText, flag, readStored, strings, useStored, writeStored } from '../stored';
import { useLatest } from '../latest';
import { useViewKeys } from '../mainKeys';
import { useStartChoice, useStartPicks } from '../useStartChoice';
import { findFor } from '../threadFind';
import { useTextSearch } from '../useTextSearch';
import { cardHeight, HEIGHT, LISTEN_HEIGHT, MARGIN, useQuickSize } from './useQuickSize';
import { keysIn } from '../platform';
import { useKeysShown } from '../keysShown';
import { onQuickBoxKey, useQuickKeys, type PickerKind } from '../quickKeys';
import { useFolderLook } from '../useFolderLook';
import { OPTIONS_FOR_MS, useRemoteOptions } from '../useRemoteOptions';
import { useTallest } from './useTallest';
import { ListenAsked, ListenRow, QuickNotes, StartRow, TodoStrip } from './QuickBits';
import { GroupLabel, GroupNotes, QuickRow, SuggestedStandIn, type RowsShown } from './QuickRows';
import { COMPOSE, deskPin, deskTitle, rowSections, suggestRows, type MoreRow, type Row, type SendToRow } from '../quickRows';

declare global {
  interface Window {
    /** Opens the panel for what Codealong asks, with the board, and returns its height once it's
     *  laid out (quick.rs calls it). */
    __quickOpen?: (context: QuickOpen | null, board: Board | null) => number;
    /** An open that came before the page was up, for the page to do once it is. */
    __quickPending?: [QuickOpen | null, Board | null];
    /** Folds the panel to the chip that holds your prompt, and returns the size its window is to
     *  be, or null when there's no prompt to hold (quick.rs `put_aside` calls it). */
    __quickFold?: () => [number, number] | null;
  }
}

const PROJECT_KEY = 'codealong.quick.project';
const HISTORY_KEY = 'codealong.quick.history';
/** What you'd typed and not acted on, so a relaunch (a new build) doesn't lose it. */
const DRAFT_KEY = 'codealong.quick.draft';
/** Whether a search lists archived sessions too, after the others that match as closely. */
const ARCHIVED_KEY = 'codealong.quick.archived';
/** How Listen now was last asked: what its session does with what it hears, and what's heard
 *  (listening.ts `ListenKept`). */
const LISTEN_KEY = 'codealong.quick.listen';
/** Don't suggest this again, ticked in Switch to Codealong Dev? (DevSetupDialog.tsx). */
const NO_DEV_KEY = 'codealong.quick.no-dev-setup';
/** The input grows to this many lines, then scrolls. */
const INPUT_LINES = 8;

/** Where in the box a press that's being heard shows (dictation.rs `Talk::id`): the open of the
 *  panel it began in, and what was selected in the box then. */
interface Heard {
  id: number;
  opens: number;
  start: number;
  end: number;
}

/** The most sessions a search lists once you ask for all of them. */
const FOUND_ALL = 40;

/** How long typing has to pause before what's typed goes to Jev. */
const ROUTE_AFTER_MS = 350;

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
  // Folded to the chip that holds your prompt while another window has the keyboard
  // (`__quickFold`); the open that unfolds it brings the board, so it isn't followed meanwhile.
  const [folded, setFolded] = useState(false);
  const foldedNow = useLatest(folded);
  const foldChip = useRef<HTMLDivElement>(null);
  // An open that unfolds the panel leaves the caret where it was (useQuickSize.ts).
  const caretKept = useRef(false);
  // Counts opens; each one is answered with the page's height once it's laid out.
  const [opens, setOpens] = useState(0);
  // How tall the window can be on its screen; the card keeps within it (useQuickSize.ts).
  const [room, setRoom] = useState<number | null>(null);
  const [opener, setOpener] = useState<string | null>(null);
  const [project, setProject] = useState<string>(() => readStored(PROJECT_KEY, asText, ALL_PROJECTS));
  const [text, setText] = useState('');
  const [attached, setAttached] = useState<Attachment[]>([]);
  // The worktree picked or asked for, or a repo picked in the repo menu (`REPO` and its key):
  // where new work goes in it, which follows the place picked (quick.ts `startLaneIn`).
  const [laneId, setLaneId] = useState<string | null>(null);
  // A folder picked in the worktree menu that's being put on the board (newfolder.rs): until the
  // board here has its worktree, the panel waits for it rather than start somewhere else. It
  // holds while the worktree picked is the one it was picked over (`from`), or the one it
  // answered; each pick and each fresh open is a new turn, and an answer to an earlier one picks
  // nothing.
  const [adding, setAdding] = useState<{ name: string; from: string | null; laneId: string | null } | null>(null);
  const addTurn = useRef(0);
  const addingNow = adding && laneId === (adding.laneId ?? adding.from) ? adding : null;
  /** Forgets a folder being added: another project or Mac is picked, where it isn't. */
  const dropAdding = () => {
    addTurn.current += 1;
    setAdding(null);
  };
  // The account or Auto, model, effort and place picked for a new session, and whether it gets a
  // new worktree (useStartChoice.ts).
  const startPicks = useStartPicks();
  const { setEffort, newWorktree, setNewWorktree, setAgent, setPlace } = startPicks;
  // The Mac a new session starts on, by its short id; null is this one.
  const [device, setDevice] = useState<string | null>(null);
  const [devices, setDevices] = useState<DevicesView | null>(null);
  // The todos a new session is for: one, or several started together on one prompt.
  const [todoIds, setTodoIds] = useState<string[]>([]);
  // Improve Codealong (improve.ts): the pictures of the windows the engine took as it opened the
  // panel, which go into the prompt; null when the panel is open for anything else.
  const [improve, setImprove] = useState<{ shots: Attachment[] } | null>(null);
  const improveNow = useLatest(improve);
  // Opened for Improve Codealong in the installed app: Switch to Codealong Dev? is asked first
  // (DevSetupDialog.tsx), until it's answered; once you say not to again (`NO_DEV_KEY`), no open
  // asks it.
  const [devAsk, setDevAsk] = useState(false);
  // While ↑ shows a draft written over Improve's pictures: the pictures the panel had before it,
  // which walking on brings back, and where it was starting, when it wasn't open for Improve.
  const recalledOver = useRef<{ improve: { shots: Attachment[] } | null; place: { project: string; laneId: string | null; device: string | null; asChat: boolean } | null } | null>(null);
  // Opened by the chat shortcut: on Codealong's chats, which the repo picker calls General chat,
  // until another is picked, and the hint names that shortcut.
  const [asChat, setAsChat] = useState(false);
  // Opened to listen (`[shortcuts] listen`, Listen now in the windows): the box asks what the
  // meeting is, nothing is listed, and ⏎ has Codealong hear it from then on, with a session
  // started for it, in a chat of its own until a worktree is picked.
  const [listen, setListen] = useState(false);
  const listenWas = useLatest(listen);
  // How it was last asked, kept for the next meeting.
  const [listenHow, setListenHow] = useStored(LISTEN_KEY, listenKept, LISTEN_KEPT);
  // What this Mac can hear with, and the meetings in the day ahead, read as the panel opens to
  // listen; and the one of them you said this isn't, by its id.
  const [hearing, setHearing] = useState<HearingStatus | null>(null);
  const [meetings, setMeetings] = useState<MeetingAhead[]>([]);
  const [notMeeting, setNotMeeting] = useState<string | null>(null);
  // The meeting it was opened for, picked in your meetings, whatever its time (`nowMeeting`).
  const [askedMeeting, setAskedMeeting] = useState<MeetingAhead | null>(null);
  // What's being heard with the key held to talk, or the microphone beside the box, while the
  // panel is the window its words are for (dictation.rs): the words themselves show in the box
  // as they're heard (talk.ts), and the microphone says it's listening and how loud. Opened
  // only to hear it, from another app, the panel goes away again when nothing was said.
  const [talk, setTalk] = useState<Talk | null>(null);
  const talking = talk !== null && (talk.phase === 'listening' || talk.phase === 'settling');
  const openedToHear = useRef(false);
  // The press whose words show in the box: which press, the open it began in, and what was
  // selected in the box as it began, which what's said is typed over. None while what's heard is
  // for another field of the panel's, or nothing is. Kept in a ref too, for what hears the
  // engine between two draws.
  const [heardAt, setHeardAt] = useState<Heard | null>(null);
  const heardRef = useRef<Heard | null>(null);
  const setHeard = (at: Heard | null) => {
    heardRef.current = at;
    setHeardAt(at);
  };
  const [error, setError] = useState<string | null>(null);
  // Whether a start in the background or in Codealong shows its session as it starts.
  const [showOn, flipShow] = useShowStarted(board?.showStarted, setError);
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
  const root = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const textNow = useLatest(text);
  const attachedNow = useLatest(attached);
  const fullPrompt = withAttachments(text, attached);
  const fullPromptNow = useLatest(fullPrompt);
  // A prompt you've written and not sent, not a word or two of a search: losing the keyboard
  // folds the panel to a chip that holds it, where it would otherwise hide (quick.rs `put_aside`).
  const holdsPrompt = holdsAPrompt(text, attached.length);
  const holdsPromptNow = useLatest(holdsPrompt);
  const loadPrompt = (raw: string): string => {
    const next = splitAttachments(raw);
    setText(next.text);
    setAttached(next.attachments);
    return next.text;
  };
  const clearPrompt = () => {
    setText('');
    setAttached([]);
    setSendTo(null);
  };
  const cachedOptions = useRef<{ at: number; options: StartOptions } | null>(null);
  // Jev's answers by project and text, so going back to what you typed doesn't ask again. Each
  // open starts afresh: sessions come and go.
  const routes = useRef(new Map<string, Route>());
  // What you'd typed when you last closed the panel without acting on it. Every open starts empty,
  // so the list is what needs you and ⏎ goes to it; ↑ in the empty input brings the draft back.
  // It's kept on every change as the next open would leave it (below), so a relaunch brings it
  // back too, with Improve Codealong's pictures when it was written over them.
  const [draft, setDraft] = useState<QuickDraft | null>(() => readStored(DRAFT_KEY, draftCodec, null));
  // Prompts you sent, started or saved, newest first, kept across restarts. ↑ walks back through
  // the draft and then these; `recall` is where it is, and holds while the text is what it
  // brought back.
  const [history, setHistory] = useStored<string[]>(HISTORY_KEY, strings, []);
  const [withArchived, setWithArchived] = useStored(ARCHIVED_KEY, flag, false);
  // Send to: what's typed in the box while it finds the session the prompt goes to, which waits
  // under it as written; null while the box holds the prompt.
  const [sendTo, setSendTo] = useState<string | null>(null);
  const inSendTo = sendTo !== null;
  const inSendToNow = useLatest(inSendTo);
  const opensNow = useLatest(opens);
  // What the box shows while a press is heard into it: what it holds, with the words so far
  // where they'll be typed, in place of what was selected. Only shown: `text` is as it was, so
  // nothing is looked for, kept as a draft or lost on what's still changing, and what's said is
  // typed over the selection itself once it's settled (one Undo takes it out).
  const hearingHere = heardAt && talking && talk?.id === heardAt.id && heardAt.opens === opens && !inSendTo ? heardAt : null;
  const heardWords = hearingHere && talk ? saidAfter(text.slice(0, hearingHere.start), talk.text) : '';
  const boxText = hearingHere && heardWords ? text.slice(0, hearingHere.start) + heardWords + text.slice(hearingHere.end) : text;
  // The words heard are selected while they show, and what was selected before them is again
  // once they go (a field whose value is set loses its selection).
  const heardShown = useRef<Heard | null>(null);
  useLayoutEffect(() => {
    const el = input.current;
    if (!el) return;
    if (hearingHere && heardWords) {
      el.setSelectionRange(hearingHere.start, hearingHere.start + heardWords.length);
      heardShown.current = hearingHere;
    } else if (heardShown.current) {
      el.setSelectionRange(heardShown.current.start, heardShown.current.end);
      heardShown.current = null;
    }
  }, [hearingHere, heardWords]);
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
  // Whether you've scrolled the list since you last chose a row or it started afresh: the
  // selected row is kept in view only until then (below, where `active` is worked out).
  const scrolledByYou = useRef(false);

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
      caretKept.current = !!ctx.keep && foldedNow.current;
      openedToHear.current = !!ctx.talk;
      setFolded(false);
      setPicker(null);
      setRoom(ctx.room ?? null);
      if (!ctx.keep) {
        routes.current.clear();
        setOpener(ctx.opener ?? null);
        setError(null);
        // A folder still being added is no longer what's picked.
        addTurn.current += 1;
        setAdding(null);
        // Improve Codealong (improve.ts) opens on Codealong's own project and worktree, as the
        // board has them, with the pictures the engine took of the windows.
        const improving = ctx.improve && typeof ctx.improve === 'object' ? ctx.improve : null;
        setImprove(improving);
        setDevAsk(!!improving && !readStored(NO_DEV_KEY, flag, false));
        recalledOver.current = null;
        improveFollowed.current = false;
        // Listening starts a chat of its own too, until a worktree is picked for it.
        setListen(!!ctx.listen);
        setNotMeeting(null);
        setAskedMeeting(ctx.listen ? (ctx.meeting ?? null) : null);
        // Opened for a meeting picked elsewhere: what an earlier open read of the day ahead
        // said of it (that it was being heard, say) may be so no longer, and only this open's
        // read is to say otherwise (`nowMeeting`).
        if (ctx.listen && ctx.meeting) setMeetings([]);
        setAsChat(!!ctx.chat || !!ctx.listen);
        const at = improving ? (b?.improve?.laneId ?? null) : (ctx.laneId ?? null);
        // The chat shortcut opens on General chat for this open alone, the project picked before
        // being where the next opens.
        setProject(improving && b?.improve?.project ? b.improve.project : ctx.chat ? CHATS : (ctx.project ?? readStored(PROJECT_KEY, asText, ALL_PROJECTS)));
        setLaneId(at);
        setTodoIds(ctx.todos ?? (ctx.todo ? [ctx.todo] : []));
        // What was typed to name a meeting is no prompt to keep as a draft.
        const typed = listenWas.current ? '' : fullPromptNow.current;
        const typedFor = todoIdsNow.current;
        const typedOver = improveNow.current?.shots;
        setDraft((d) => nextDraft(typed, typedFor, d, typedOver));
        loadPrompt(ctx.prompt ?? '');
        setSendTo(null);
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
      // Your other machines, for starting there and for a search to find their sessions, when
      // there are any; while the panel is up it follows them.
      if (b?.devices?.enabled)
        getDevices()
          .then((d) => {
            if (d) setDevices(d);
            clearNotice('quick-devices');
          })
          .catch((e: unknown) => reportNotice(failedNotice('quick-devices', 'Can’t read your other devices', 'The quick panel couldn’t read your other machines, so it can’t start sessions there', e)));
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
    // Opened by Codealong's script: laid out and measured before it returns. Its board is the one
    // the next push changes, so the panel doesn't read it again for that.
    window.__quickOpen = (ctx, b) => {
      if (b) boardSeen(b);
      openNow('script', ctx ?? {}, b);
      return reported.current;
    };
    // Folded by Codealong as another window takes the keyboard: the chip is drawn before this
    // returns, where the card's top was, and the window shrinks round it after.
    window.__quickFold = () => {
      if (!holdsPromptNow.current) return null;
      flushSync(() => {
        setFolded(true);
        setPicker(null);
      });
      const r = foldChip.current?.getBoundingClientRect();
      return r ? [Math.ceil(r.width) + 2 * MARGIN, Math.ceil(r.height) + 2 * MARGIN] : null;
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
        setFolded(false);
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
      delete window.__quickFold;
      offHidden();
    };
    // Once: the refs and setters are the same objects on every render.
  }, [fullPromptNow, todoIdsNow, improveNow, listenWas, foldedNow, holdsPromptNow, setAgent, setEffort, setPlace, setNewWorktree]);

  // Opened to listen: what this Mac can hear with, for what macOS will ask, and the meetings in
  // the day ahead, for the one going on. Not said when a read fails: Listen now then offers no
  // meeting from the calendar and says nothing of what macOS will ask, and listens all the same.
  useEffect(() => {
    if (!shown || !listen) return;
    let over = false;
    meetingsAhead().then(
      (all) => !over && setMeetings(all),
      () => {},
    );
    hearingStatus().then(
      (h) => !over && setHearing(h),
      () => {},
    );
    return () => {
      over = true;
    };
  }, [shown, listen, opens]);

  useEffect(
    () =>
      shown && !folded
        ? onBoard((b) => {
            arrived.current = performance.now();
            setBoard((prev) => (prev ? reuse(prev, b) : b));
          })
        : undefined,
    [shown, folded],
  );
  useEffect(() => (shown ? onDevices(setDevices) : undefined), [shown]);
  usePerf(engineBoard, arrived);
  // Your other machines while the board says devices are on: put away, the panel doesn't hear
  // them turned off, and the view it read last still says they're on.
  const devicesOn = engineBoard?.devices?.enabled ? devices : null;
  // Their worktrees, with what any window changed of their sessions that the engine hasn't shown
  // yet, as this machine's board has them (useDevices.ts).
  const deviceLanes = useDeviceLanes(devicesOn, ahead);
  // The panel follows the board only while it's up; so does what it knows of its changes, by
  // both halves.
  useObserve(engineBoard, devicesOn, shown && !!engineBoard);

  const theme = board?.theme ?? 'system';
  useTheme(theme);
  useGlass(!!board?.glass);
  useKeysShown(board?.showKeys);
  const logoStatus = useLogo(board?.logoStatus);

  // The input grows with what you write. At its tallest with more than it shows there's nothing
  // to measure: setting it to auto to measure laid a long prompt out again at every key.
  useLayoutEffect(() => {
    const el = input.current;
    if (!el) return;
    const line = parseFloat(getComputedStyle(el).lineHeight) || 22;
    const tallest = Math.ceil(line * INPUT_LINES);
    if (el.style.height === `${tallest}px` && el.scrollHeight > el.clientHeight) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, tallest)}px`;
  }, [boxText, sendTo, opens]);

  // Not out of a menu that opened since: a press on another menu's button closes the one open,
  // which asks for this, and its click can open the other before the frame comes.
  const refocus = () => window.requestAnimationFrame(() => document.activeElement?.closest('.ca-picker') || input.current?.focus());

  /* ---- Where ---- */

  const projects = useMemo(() => board?.projects ?? [], [board?.projects]);
  const projectIcons = useMemo(() => projectIconMap(projects), [projects]);
  // The board lists no projects until it's read, so the one picked holds until then. General
  // chat is no project of the board's: it lists the chats, and starts the next one there.
  const current = project === ALL_PROJECTS || project === CHATS || projects.some((p) => p.name === project) || !board || board.loading ? project : ALL_PROJECTS;
  const chatting = current === CHATS;
  /** A project picked, kept for the next open unless `keep` is false (the chat shortcut's). */
  const pickProject = (name: string, keep = true) => {
    if (name !== current) dropAdding();
    // General chat starts in Codealong's chats, whatever worktree was picked before it; leaving it
    // for a project starts in that project's repos again, not in the chats. (The chat shortcut's
    // chat stays one under whatever project is picked.)
    if (name !== current && (name === CHATS || (chatting && lane?.startsChats))) {
      setLaneId(null);
      setAsChat(false);
    } else if (name !== current) {
      // A worktree of the project left doesn't come along: the start goes to the new one's
      // first repo, so the repo menu lists that project's alone (quick.ts `repoForProject`).
      // Picked by its repo, which is what moves Improve Codealong off its source too.
      setLaneId(pickInProject(laneId, lane, lanesOn, name, pinnedWorktrees ?? [], REPO));
      // A project picked is a pick since a skill moved the start, as a worktree picked is: what
      // the skill's repo took the place of doesn't come back once the skill's gone from the input.
      beforeSkill.current = null;
    }
    setProject(name);
    setActiveKey(null);
    setPicker(null);
    if (keep) writeStored(PROJECT_KEY, asText, name);
    refocus();
  };
  // The chat shortcut pressed with the panel up (quick.rs `toggle_chat`): to General chat, for
  // this open alone as when it opens the panel, its menu put away if it was up; pressed there, it
  // puts the panel away.
  const chatKey = useLatest(() => {
    if (chatting) {
      void hideQuick();
      return;
    }
    pickProject(CHATS, false);
    setAsChat(true);
  });
  useEffect(() => onQuickChat(() => chatKey.current()), [chatKey]);
  const counts = useMemo(() => (board ? projectCounts(board, [...projects.map((p) => p.name), CHATS, ALL_PROJECTS]) : {}), [board, projects]);

  // Which Mac: this one, or another linked Mac, which starts the session with its own accounts
  // in its own worktrees. Until your devices are read, one a window asked for is taken as it is.
  const macs = useMemo(() => (devices?.enabled ? devices.devices : []), [devices]);
  const deviceNow = device && (!devices || macs.some((d) => d.short === device)) ? device : null;
  const deviceInfo = macs.find((d) => d.short === deviceNow) ?? null;
  // Where its accounts can start is asked of it once it's chosen, and asked again once that's a
  // minute old (useRemoteOptions.ts).
  const { remote, askAgain } = useRemoteOptions(shown, deviceNow);
  const startOptionsNow = deviceNow ? (remote?.options ?? null) : options;

  // Where a session can start: the project's worktrees on that Mac, and one of another project
  // that a window, a todo or a skill asked for, or that was picked on General chat, with its
  // repo's, which its worktree menu lists (picking another project moves the start into that
  // project: `pickProject`). Codealong's chats are apart from them, as General chat, whatever the
  // project; on General chat there are no worktrees but one picked. `lanesOn`: every project's
  // on that Mac, its chats too.
  const { lanes, chatLane, lanesOn } = useMemo(() => {
    const all = deviceNow ? (devices?.lanes ?? []).filter((l) => shortOf(l.id) === deviceNow) : (board?.lanes ?? []);
    const worktrees = all.filter((l) => !l.startsChats);
    const mine = current === CHATS ? [] : worktrees.filter((l) => inProject(l, current));
    const isAsked = (l: Lane) => l.id === laneId || `${REPO}${repoKey(l)}` === laneId;
    const asked = laneId && !mine.some(isAsked) ? worktrees.find(isAsked) : undefined;
    const extra = asked ? worktrees.filter((l) => repoKey(l) === repoKey(asked)) : [];
    return { lanes: [...mine, ...extra], chatLane: all.find((l) => l.startsChats) ?? null, lanesOn: all };
  }, [board?.lanes, devices?.lanes, deviceNow, current, laneId]);
  const pinnedWorktrees = board?.pinnedWorktrees;
  const repoList = useMemo(() => repoChoices(lanes, pinnedWorktrees ?? []), [lanes, pinnedWorktrees]);

  const shownNow = useLatest(shown);

  /* ---- A skill, which picks its repo ---- */

  // A slash and a name being typed at the start of the text list the skills of the project's
  // repos on this Mac and the accounts' own; the skill it names whole is the one a start runs,
  // and a repo's is where. A todo's prompt is words, and another Mac's folders and accounts
  // aren't read from here.
  const mentions = useMemo(() => (todoIds.length === 0 && !deviceNow && !listen ? mentionsIn(text) : []), [text, todoIds.length, deviceNow, listen]);
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
      // worktree, or one or a project you picked since (`pickProject`), stays.
      if (beforeSkill.current && laneIdNow.current === beforeSkill.current.put) {
        setLaneId(beforeSkill.current.laneId);
        setNewWorktree(beforeSkill.current.newWorktree);
      }
      beforeSkill.current = null;
      return;
    }
    const to = skillStart(repo, lanesNow.current);
    // Where new work goes in its repo is the repo's own pick, which follows the place as a repo
    // picked in its menu does.
    const ofRepo = lanesNow.current.filter((l) => repoKey(l) === repoKey(to.lane));
    const put = !to.newWorktree && startLaneIn(ofRepo, false)?.id === to.lane.id ? `${REPO}${repoKey(to.lane)}` : to.lane.id;
    beforeSkill.current = { laneId: laneIdNow.current, newWorktree: newWorktreeNow.current, ...beforeSkill.current, put };
    setLaneId(put);
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
  // Without a worktree asked for, where new work starts in the repo picked, else the first on the
  // list: one with a pinned worktree, else the latest (quick.ts `startLaneIn`), the main
  // checkout where the place picked (`isolates`) moves the session out of it before it changes
  // anything.
  // Opened by the chat shortcut or on General chat, the chats lane, once the board has it: a
  // panel opened before the first scan finished gets it with the board that has it.
  // Improving Codealong with no source on the board goes to no worktree by default (the note
  // says why), never to the project's first, which would start it in some other repo, nor to
  // General chat's chats.
  const repoPicked = laneId?.startsWith(REPO) ? repoList.find((r) => `${REPO}${r.key}` === laneId) : undefined;
  const laneWith = (isolates: string[]) =>
    lanes.find((l) => l.id === laneId) ??
    (chatLane && (chatLane.id === laneId || ((asChat || (chatting && !improve)) && !laneId)) ? chatLane : null) ??
    (improve && !repoPicked ? null : startLaneIn((repoPicked ?? repoList[0])?.lanes ?? [], false, isolates));
  // The account, model, effort and place a new session starts with (useStartChoice.ts), Auto
  // from the group the start's project names: a repo's worktrees are all in its project, so
  // whichever the place picks.
  const {
    accounts,
    groups: routingGroups,
    models,
    efforts,
    pick,
    agents,
    first,
    agentNow,
    autoFor,
    accountInfo,
    account,
    harness,
    places,
    placeNow,
    makesWorktree: harnessMakesWorktree,
    codealongMakesWorktree,
    inNewWorktree: newWorktreePicked,
    isolatesIn,
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
    pickEffort,
    effortShown,
  } = useStartChoice(startPicks, optionsForStart, usage, now, allLanes, laneWith([])?.project ?? null);
  const lane = laneWith(isolatesIn);
  // The repo it starts in, whose worktrees the worktree menu lists.
  const repoNow = lane && !lane.startsChats ? (repoList.find((r) => r.key === repoKey(lane)) ?? null) : null;
  // While the machine a start goes to is strained, a calm one with its repo (or chats), which
  // ⇧⌘D or the note's button switches to (machineLoad.ts): never by itself. Not in Send to, nor
  // for what starts on this machine alone (listening, improving Codealong, a prompt that starts
  // with a skill's name, known here or still being looked up, which another machine's start would
  // take as words).
  const startsOnName = deviceInfo?.name ?? (devices?.me.name || 'This machine');
  const startsOnLoad = deviceNow ? deviceInfo?.load : board?.load;
  const calmer = devicesOn && !inSendTo && !listen && !improve && !skillNow && !text.trimStart().startsWith('/') ? calmerMachine({ on: deviceNow, lane }, { name: devices?.me.name || 'This machine', load: board?.load, lanes: board?.lanes ?? [] }, macs, devices?.lanes ?? []) : null;
  /** A suggested start's agent as it would start, among `only` when it needs a service only they
   *  reach (route.ts `resolveSuggestedAgent`). With none suggested, the start row's choice, but
   *  Auto in another project (`at`'s) starts from the group that project has for it
   *  (`autoGroupIn`); null where that's the start row's own. */
  const choiceForAgent = (suggestedAgent: string | undefined, overWorktree: boolean, only: string[] | null = null, at: Lane | null = null) => {
    const picks = { agent: startPicks.agent, effort: startPicks.effort, place: placeNow, newWorktree: overWorktree };
    if (!suggestedAgent) {
      const there = at && isAuto(choice) ? autoGroupIn(optionsForStart, at.project) : null;
      if (there === null || there === autoGroupIn(optionsForStart, lane?.project)) return null;
      const pickThere = autoPick(accounts, usage, now, null, routingGroups.find((g) => g.name === there) ?? null, allLanes);
      // Only a place you picked goes along: the row's is its own account's, and another
      // account opens where its settings say.
      return startChoice(accounts, models, efforts, usage, pickThere, agents, first, { ...picks, place: startPicks.place }, () => undefined, routingGroups, now, allLanes);
    }
    return suggestedChoice(suggestedAgent, accounts, models, efforts, usage, pick, agents, first, picks, () => undefined, routingGroups, now, allLanes, only);
  };
  // A chat has no repo to make a worktree of.
  const makesWorktree = harnessMakesWorktree && !lane?.startsChats;
  const inNewWorktree = newWorktreePicked && makesWorktree;
  // Where the start goes: a new worktree is made from its repo's checkout for it (quick.ts
  // `repoWorktrees`), whichever of the repo's worktrees is picked, which is where it starts once
  // New worktree is off or the account makes none.
  const startLane = inNewWorktree && repoNow ? (startLaneIn(repoNow.lanes, true) ?? lane) : lane;

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
  // lists its worktrees and the panel moves onto them, once; or another of your Macs has it, and
  // the panel moves there once the board says this Mac has none, the pictures going with the
  // prompt (actions.rs `carry_images`).
  const remoteSource = improve && !board?.improve?.laneId ? remoteImproveLane(devices?.lanes ?? []) : null;
  const improveNote = improve ? improveWords(board?.improve, !board || board.loading, remoteSource) : null;
  const improveSource = board?.improve?.source ?? null;
  // In the installed app, Switch to Codealong Dev? first (DevSetupDialog.tsx). The session it
  // starts sets up this machine, so it goes to the source's checkout here, or to a chat of its
  // own here that clones one: never to another Mac's source, which the panel waits to move to.
  const askingDev = devAsk && !!improve && !!board?.improve?.installed;
  // An open while it's up leaves it the keyboard (useQuickSize), even one that finds it still up
  // from before the panel was put away.
  const askingDevNow = useLatest(askingDev);
  const devSourceLane = board?.improve?.laneId ? board.lanes.find((l) => l.id === board.improve?.laneId && !l.device) : undefined;
  const devSource = devSourceLane?.path ?? improveSource?.root ?? null;
  const setUpDev = (dontAsk: boolean) => {
    setDevAsk(false);
    if (dontAsk) writeStored(NO_DEV_KEY, flag, true);
    // The folders Codealong looks for repos in, for the clone; without them it's asked where.
    getConfig()
      .then((view) => view.config.scan)
      .catch(() => [])
      .then((scan) => startTask(devSetupTask(devSource, scan, devSourceLane?.id ?? null)));
  };
  // A session Codealong offers (agentTasks.ts) that's asked for here, not opened on the panel by
  // another window: it starts as a Return would, in its worktree or this machine's chats.
  const startTask = (task: AgentTask) => {
    const where = board?.lanes.find((l) => (task.laneId ? l.id === task.laneId : l.startsChats && !l.device));
    if (!where) {
      setError('There’s no place on this machine to start the session yet: try again once the board is read.');
      return;
    }
    start({ lane: where, newWorktree: false, prompt: task.prompt });
  };
  const closeDevAsk = (dontAsk: boolean) => {
    setDevAsk(false);
    if (dontAsk) writeStored(NO_DEV_KEY, flag, true);
    refocus();
  };
  // The source's worktree is taken from the board once per open: a worktree or Mac you pick
  // after that stands. Another Mac's waits for the board, which may yet find one here.
  const improveFollowed = useRef(false);
  const remoteSourceId = board && !board.loading && !board.improve?.source ? (remoteSource?.id ?? null) : null;
  const remoteSourceProject = remoteSource?.project ?? null;
  useEffect(() => {
    if (!improve || improveFollowed.current || askingDev) return;
    if (board?.improve?.laneId) {
      improveFollowed.current = true;
      setLaneId(board.improve.laneId);
      setDevice(null);
      if (board.improve.project) setProject(board.improve.project);
    } else if (remoteSourceId) {
      improveFollowed.current = true;
      setLaneId(remoteSourceId);
      setDevice(shortOf(remoteSourceId));
      if (remoteSourceProject) setProject(remoteSourceProject);
    }
  }, [improve, askingDev, board?.improve?.laneId, board?.improve?.project, remoteSourceId, remoteSourceProject]);
  const addCodealongProject = () => {
    if (!improveSource) return;
    const source = improveSource;
    setError(null);
    getConfig()
      .then((view) => saveConfig(withCodealongProject(view.config, source)))
      .then(() => setInfo(`Added the ${CODEALONG_PROJECT} project; its worktrees come onto the board in a moment.`))
      .catch((e: unknown) => setError(errorText(e)));
  };

  // Attachments go into the strip below the input rather than as markdown in the text box. A file
  // let go anywhere on the panel is attached, the chip it folds to too: Codealong keeps the panel
  // up while a file comes from another app, and brings it back with the keyboard once it lands
  // (quick.rs `lost_keys`, `dragged`).
  const addAttachments = (kept: Attachment[]) => {
    setAttached((prev) => mergeAttachments(prev, kept));
    refocus();
  };
  useFileDrop(root, addAttachments, setError);

  /* ---- The list ---- */

  // With a todo to start, the input holds its prompt, not a search: only the new session is
  // listed.
  const prompted = text.trim().length > 0 || attached.length > 0;
  // Set to listen, it holds what the meeting is: nothing is looked for, sent to Jev or listed.
  const query = todo || slash || listen ? '' : text.trim();
  // With nothing typed or attached, a few todos ready to start go under the sessions, which leave them room.
  const listing = !query && attached.length === 0 && !todo && !slash && !listen;
  const readyList = useMemo(() => (board && listing ? readyTodos(board, current) : null), [board, current, listing]);
  const attention = useMemo(() => (board && listing ? attentionRows(board, current, MAX_ROWS - (readyList?.rows.length ?? 0)) : null), [board, current, listing, readyList]);
  // Below them, the rest of the day's sessions, for ↓ to reach one that isn't waiting on you.
  const recent = useMemo(() => (board && attention ? recentRows(board, current, new Set(attention.rows.map((r) => r.key)), now) : null), [board, current, attention, now]);
  // A search finds your other machines' sessions too, as the main window lists them, after the
  // same match on the machine a new session would start on (quick.ts `findSessions`). Send to
  // lists this machine's only: what's typed goes to a session here.
  const machines = useMemo((): Machines => ({ lanes: deviceLanes, on: deviceNow }), [deviceLanes, deviceNow]);
  // What was said in sessions is searched in the engine, once typing pauses, and on your other
  // machines in theirs; its hits join the list below the sessions whose titles and the like
  // match. In Send to, it's what's typed in the box that's looked for, and the prompt's own
  // search waits.
  const { found: textFound, said } = useTextSearch(inSendTo ? sendTo : query, { ready: !!board, devices: machines.lanes.length > 0 && !inSendTo });
  const searching = !!query && !inSendTo;
  const archivedMode = withArchived ? 'with' : 'without';
  const foundSessions = useMemo(() => (board && searching ? findSessions(board, current, query, now, FOUND_ALL, said, archivedMode, machines) : []), [board, current, query, searching, said, archivedMode, now, machines]);
  // How many archived sessions match, for the toggle that lists them or leaves them out.
  const archivedFound = useMemo(
    () => (!board || !query ? 0 : inSendTo ? sendTargets(board, current, sendTo, [], now, said, 'only', 0).count : findSessions(board, current, query, now, FOUND_ALL, said, 'only', machines).length),
    [board, current, query, inSendTo, sendTo, said, now, machines],
  );
  // A search lists its first few sessions, so todos and PRs fit below; a row under them lists
  // the rest, until what's typed changes.
  const [allFor, setAllFor] = useState<string | null>(null);
  const showAll = allFor === query;
  const foundTodos = useMemo(() => (board && searching ? findTodos(board, current, query) : []), [board, current, query, searching]);
  const foundPrs = useMemo(() => (board && searching ? findPrs(board, current, query) : []), [board, current, query, searching]);
  // What's typed is a prompt rather than a word or two of a search: it can be sent to any of the
  // project's sessions, by Send to under the sessions found (⌘F), as well as to the ones Jev
  // suggests. With routing off too.
  const sendable = useMemo(() => (board && !inSendTo && worthRouting(query) ? sendTargets(board, current, '', [], now, undefined, archivedMode, 0).count : 0), [board, current, query, inSendTo, archivedMode, now]);

  // With routing on, what you type (and any pictures attached to it) goes to Jev once you pause,
  // and its suggestions lead the list, so ⏎ does the likeliest. A word or two alone is a search
  // and isn't sent.
  const routePrompt = query ? withShots(withAttachments(query, attached)) : '';
  // Not while it's being said: the words change until the key comes up. On General chat a prompt
  // starts a chat, with no worktree for Jev to weigh, so it isn't asked and the start row leads.
  const routeFor = board?.decisions?.quick_panel && !todo && !talking && !chatting && worthRouting(routePrompt) ? `${current}\n${routePrompt}` : null;
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
  // Where the place a suggested start opens in moves the session out of the main checkout: that
  // of the agent Jev's starts go to (route.ts `startAgentOf`), else the start row's. Kept by what
  // the checkouts are, not by the list, which is a new one at each render when the place has none.
  const isolatedFor = (r: Route | null) => {
    const to = r ? startAgentOf(r) : {};
    const c = to.agent ? choiceForAgent(to.agent, false, to.needs?.accounts) : null;
    return (c ? c.isolatesIn : isolatesIn).join('\n');
  };
  const isolated = isolatedFor(route);
  const lastIsolated = isolatedFor(lastRoute);
  const jevSuggested = useMemo(() => suggestRows(route, board, !!deviceNow, isolated), [board, route, deviceNow, isolated]);
  const jevLastSuggested = useMemo(() => suggestRows(lastRoute, board, !!deviceNow, lastIsolated), [board, lastRoute, deviceNow, lastIsolated]);
  /** Whether a suggested start makes a new worktree, as `start` makes it: only where its account
   *  makes worktrees and the worktree isn't for chats. */
  const startsTree = (sg: Suggestion & { kind: 'start' }) => {
    const c = choiceForAgent(sg.agent, sg.newWorktree, sg.needs?.accounts, sg.lane);
    return (c ? c.makesWorktree : harnessMakesWorktree) && !sg.lane.startsChats && sg.newWorktree;
  };
  // A start Jev suggests that would start just what the start row does is left out: the start row
  // says how likely Jev thinks it is, and ⏎ goes to it when that start led. What a suggested
  // start starts is what `start` makes of it.
  const startRowFacts: StartFacts | null = startLane ? { laneId: startLane.id, newWorktree: inNewWorktree, account, model: modelNow, effort: effortNow, place: placeNow } : null;
  const isStartRow = (sg: Suggestion) => {
    if (sg.kind !== 'start' || !startRowFacts) return false;
    const c = choiceForAgent(sg.agent, sg.newWorktree, sg.needs?.accounts, sg.lane);
    return sameStart({ laneId: sg.lane.id, newWorktree: startsTree(sg), account: c ? c.account : account, model: c ? c.modelNow : modelNow, effort: c ? c.effortNow : effortNow, place: c ? c.placeNow : placeNow }, startRowFacts);
  };
  const startRowSuggested = jevSuggested.find((r) => isStartRow(r.s))?.s ?? null;
  const startLeads = (!!jevSuggested[0] && isStartRow(jevSuggested[0].s)) || (chatting && !todo && worthRouting(routePrompt));
  const suggested = jevSuggested.filter((r) => !isStartRow(r.s));
  const lastSuggested = jevLastSuggested.filter((r) => !isStartRow(r.s));

  // What Jev said beyond where it goes: a session that may already be on it.
  const duplicate = board ? duplicateOf(route, board) : null;
  const alreadyOn = alreadyOnWords(duplicate);

  // The Suggested group is there while what's typed goes to Jev, and only grows meanwhile: an
  // answer with fewer suggestions than the last leaves room where they were, so the rows under it
  // stay put. It lets go once what's typed no longer goes to Jev. Laid out before where the rows
  // sit is compared, below.
  const suggestedGroup = useTallest<HTMLDivElement>();

  // The panel's height, one whatever it lists, told to quick.rs as it opens (useQuickSize.ts).
  const tall = listen ? LISTEN_HEIGHT : HEIGHT;
  useQuickSize({ list, scroller, input, asking: askingDevNow, opens, room, tall, opening, reported, caretKept });

  // The last answer orders the sessions found too while the next is worked out, so the ones it
  // put first don't drop back at every key.
  const ranked = rankedBy(foundSessions, route ?? lastRoute);
  const moreRow: MoreRow[] = !showAll && ranked.length > FOUND_SESSIONS ? [{ kind: 'more', key: 'more:found', group: 'found', count: ranked.length - FOUND_SESSIONS }] : [];
  // Under Jev's suggestions while what's typed goes to Jev, else under the sessions found; there too
  // when Jev takes it for a search (the last answer's word standing while the next is worked out),
  // whose results come first.
  const asSearch = (route ?? lastRoute)?.action === 'find';
  const sendToRow: SendToRow[] = sendable > 0 ? [{ kind: 'sendto', key: 'sendto', group: routeFor && !asSearch ? 'suggested' : 'found', count: sendable }] : [];
  const sendToIn = sendToRow[0]?.group ?? null;
  // Send to's list: every session the prompt could go to, Jev's picks for it first with how
  // likely each is (the last answer's while the next is worked out, as the sessions found have
  // it), found by what's typed in the box.
  const jevPicks = (route ?? lastRoute)?.sessions;
  const sendList = useMemo(() => (board && inSendTo ? sendTargets(board, current, sendTo, jevPicks ?? [], now, said, archivedMode) : null), [board, current, inSendTo, sendTo, jevPicks, said, archivedMode, now]);
  const rows: Row[] = inSendTo
    ? (sendList?.rows ?? [])
    : slash
      ? skillsListed
      : query
        ? [...suggested, ...(sendToIn === 'suggested' ? sendToRow : []), ...(showAll ? ranked : ranked.slice(0, FOUND_SESSIONS)), ...moreRow, ...(sendToIn === 'found' ? sendToRow : []), ...foundTodos, ...foundPrs]
        : [...(attention?.rows ?? []), ...(readyList?.rows ?? []), ...(recent?.rows ?? [])];
  /** Lists archived sessions in searches, or leaves them out, from now on (Sessions' heading has
   *  the box, ⇧⌘A presses it). They rank further down than the others, so turned on, the search
   *  lists all it finds, and the ones just asked for show. */
  const toggleArchived = () => {
    if (!withArchived) setAllFor(query);
    setWithArchived(!withArchived);
  };
  // Send to sends what's typed to a session: the new session's row and its settings below are
  // put away until the box holds the prompt again.
  const keys = [...rows.map((r) => r.key), ...(inSendTo || listen ? [] : [COMPOSE])];
  // The rows ⌘1 to ⌘9 reach, in order; Send to's row has ⌘F.
  const numbered: Row[] = rows.filter((r) => r.kind !== 'sendto');
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
    // A meeting's name, typed to listen to it, is no prompt to bring back: set to listen, the
    // draft is kept as it is.
    writeStored(DRAFT_KEY, draftCodec, listen ? draft : nextDraft(fullPrompt, todoIds, draft, improve?.shots));
  }, [fullPrompt, todoIds, draft, improve, listen]);
  // For a search, ⏎ does the first thing it found, a session, todo or PR, when Jev suggests nothing.
  const firstFound = asSearch && !inSendTo ? (keys.find((k) => k !== 'sendto' && k !== COMPOSE) ?? keys[0]) : keys[0];
  const active = activeKey && keys.includes(activeKey) ? activeKey : startLeads && !inSendTo ? COMPOSE : firstFound;

  // The selected row is kept in view, whatever selects it (you, Jev's answer coming in, the row
  // that leads changing), until you scroll the list yourself; then only your next choice or a
  // fresh list (what's typed, Send to's box, the project, an open) moves it. Following the row
  // that leads as the list sorts itself again under a list you'd scrolled down (a session that
  // works moving up, what sessions said coming in) took you back to the top every few seconds.
  // It's found by its key, not its class: rows a render left behind (two groups under one key,
  // 2026-11-04) kept `is-active` and took the scroll to the top.
  useEffect(() => {
    scrolledByYou.current = false;
  }, [activeKey, text, sendTo, current, opens]);
  useEffect(() => {
    if (scrolledByYou.current) return;
    const rows = list.current?.querySelectorAll<HTMLElement>('[data-row]') ?? [];
    Array.from(rows)
      .find((el) => el.dataset.row === active)
      ?.scrollIntoView({ block: 'nearest' });
  }, [active, activeKey, text, sendTo, current, opens]);

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

  /** Opens a session where it lives. The search that found it is done with. One on another
   *  machine opens there, not from here, so Codealong shows it, as its header has no open
   *  (SessionHeader.tsx). */
  const openSession = (lane: Lane, session: Session) => (lane.device ? showSession(lane, session) : openHere(lane, session.openTarget));

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

  const revealSession = (lane: Lane, session: Session, find?: string) => {
    void hideQuick();
    void showMain({ kind: 'session', laneId: lane.id, key: session.key, find });
  };
  // Holding a prompt, the panel folds as the main window takes the keyboard, rather than hiding.
  useViewKeys(() => {
    if (!holdsPromptNow.current) void hideQuick();
  });

  /** What picking a session does: Codealong shows it, with its conversation, rather than the
   *  terminal or app it runs in. The search that found it is done with, as when it opens there;
   *  one that found it by what was said in it has the conversation's find bar open on its words
   *  (`find`). */
  const showSession = (lane: Lane, session: Session, find?: string) => {
    clearPrompt();
    setActiveKey(null);
    setError(null);
    revealSession(lane, session, find);
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

  /** Answers a session in the waiting window, which lists another machine's only while it's
   *  connected and `[devices] in_bar` is on (devices.ts `floatingLanes`); one it doesn't list,
   *  a search may still find, and Codealong shows it instead. Renaming is the waiting window's
   *  alone here, so the menu offers it only where that lists the session (`quickCan`). */
  const inWaiting = (lane: Lane) => !lane.device || floatingLanes(board?.devices, [lane]).length > 0;
  const answerSession = (lane: Lane, session: Session) => (inWaiting(lane) ? answerIn(session.key) : showSession(lane, session));

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

  const revealPr = (pr: PrItem, tab?: PrTab | null) => {
    void hideQuick();
    void showMain({ kind: 'pr', key: pr.key, tab: tab ?? undefined });
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
  // Mark as done's question about helpers whose work isn't over (menus.ts `doneHelpers`).
  const askingHelpers = useDoneHelpers();

  /** What the panel can do to a session (menus.ts), for its menu. Reply here and Rename… go to
   *  the waiting window. */
  const quickCan = (lane: Lane, s: Session): menus.SessionCan => ({
      doneHelpers: board ? { lanes: board.lanes, ask: askingHelpers.ask } : undefined,
      report: menuReport,
      open: openHere,
      terminal: board?.apps.terminal ?? 'Terminal',
      reveal: () => revealSession(lane, s),
      reply: () => answerSession(lane, s),
      rename: inWaiting(lane) ? () => answerIn(s.key, true) : undefined,
      labels: board?.labels,
      worktrees: board?.lanes,
      editors: board ? { apps: board.apps, icons } : undefined,
      desktop: onDesktop,
  }) satisfies menus.CanIn<'quick'>;
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
    if (r.kind === 'more' || r.kind === 'sendto' || r.kind === 'skill') return;
    const show = (spec: MenuSpec) => void showContextMenu(spec).catch((e: unknown) => setError(errorText(e)));
    // What's typed going somewhere from a menu is noted as ⏎ on the row would note it.
    const noted = (did: RouteChoice, via: RouteNote['via'], run: () => void) => () => {
      noteDone(did, via);
      run();
    };
    if (r.kind === 'suggest') {
      const sg = r.s;
      if (sg.kind === 'send' || sg.kind === 'open') {
        show([...(sg.kind === 'send' ? [{ label: 'Send this prompt to it', run: noted(routeDone(sg, false), 'suggested', () => send(sg.session)) }, 'separator' as const] : []), ...sessionMenuSpec(sg.lane, sg.session)]);
      } else {
        const tree = sg.kind === 'start' && startsTree(sg);
        // A todo links where one Jev suggests would: a start in a repo's free worktree is the repo's.
        const todoLane = sg.kind === 'start' ? todoLaneFor(sg.lane, allLanes) : sg.lane;
        show([
          { label: `Start a session in ${sg.lane.callsign}`, run: noted({ kind: 'start', target: sg.lane.id, newWorktree: tree }, 'suggested', () => start({ lane: sg.lane, newWorktree: sg.kind === 'start' && sg.newWorktree, agent: sg.kind === 'start' ? sg.agent : undefined, only: sg.kind === 'start' ? sg.needs?.accounts : undefined })) },
          { label: `Save as a todo in ${todoLane.project}`, run: noted({ kind: 'todo', target: todoLane.id }, 'suggested', () => saveTodo(todoLane)) },
        ]);
      }
      return;
    }
    if (r.kind === 'session') {
      // In Send to, what ⏎ does leads, as on a suggestion's menu.
      const sg = r.group === 'sendto' ? toSessionSuggestion(r.lane, r.session, r.p ?? 0) : null;
      show([...(sg?.kind === 'send' ? [{ label: 'Send this prompt to it', run: noted(routeDone(sg, false), 'send-to', () => send(sg.session)) }, 'separator' as const] : []), ...sessionMenuSpec(r.lane, r.session)]);
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
  const onModel = onModelWords(startsOn, effortWord, ownModels(accountInfo, models));
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

  /** Keeps a place as its accounts' `start` in config.toml (`keepPlaceFor`), so their next
   *  starts open there, from here or from `codealong start`; and in the copy of where accounts can
   *  start, so the next open goes by it without reading again. Only Make default does it: a place
   *  picked for a start is for that start alone (asked 2026-10-19). */
  const keepPlace = (names: string[], p: StartPlace) => {
    if (names.length === 0) return;
    keepStartPlace(names, p)
      .then(() => {
        const cached = cachedOptions.current;
        if (cached) cachedOptions.current = { ...cached, options: withChosen(cached.options, names, p) };
        setOptions((o) => o && withChosen(o, names, p));
      })
      // Asked for here, so said here.
      .catch((e: unknown) => setError(keepPlaceFailed(e)));
  };

  /** Keeps what you did with what you typed beside what Jev suggested for it, to measure how often
   *  its first suggestion is what you did (routelog.rs). Only for what went to Jev. */
  const noteDone = (did: RouteChoice, via: RouteNote['via']) => {
    if (!routeFor) return;
    const at = routeFor.indexOf('\n');
    const note = { project: routeFor.slice(0, at), text: routeFor.slice(at + 1), suggested: route ? jevSuggested.map((r) => logged(routeChoice(r.s), r.s)) : null, did, via };
    // A note that isn't kept only leaves this one out of the measure; the engine says why as a
    // notice when the file won't take it.
    noteRoute(note).catch(() => {});
  };

  /** A suggestion's start as `start` makes it, in a new worktree or not (`startsTree`), so a
   *  start from the new session's row compares with it. */
  const logged = (c: RouteChoice, sg: Suggestion): RouteChoice => (sg.kind === 'start' ? { ...c, newWorktree: startsTree(sg) } : c);

  /** What a row did, for the routing log: the new session's row is noted as it starts. Its
   *  menu's items that take what's typed are noted as they run (`rowMenu`). */
  const noteRow = (row: Row, alt: boolean) => {
    if (row.kind === 'suggest') noteDone(logged(routeDone(row.s, alt), row.s), 'suggested');
    else if (row.kind === 'session') noteDone(row.group === 'sendto' ? routeDone(toSessionSuggestion(row.lane, row.session, 0), alt) : { kind: 'open', target: row.session.key }, row.group === 'sendto' ? 'send-to' : 'found');
    else if (row.kind === 'todo' || row.kind === 'pr' || row.kind === 'review') noteDone({ kind: 'other', target: null }, 'found');
  };

  /** Starts a session: in the worktree chosen below, or where a suggestion says; on what's typed,
   *  or on a `prompt` of the panel's own (Switch to Codealong Dev?'s), which nothing is added to.
   *  `flip` (⌥) shows it as it starts, or doesn't, the other way from the switch, this once. */
  const start = (over?: { lane: Lane; newWorktree: boolean; agent?: string; only?: string[]; prompt?: string }, flip = false) => {
    // Not while a press is heard into the box: what it shows isn't what it holds yet.
    if (hearingHere) return;
    const where = over?.lane ?? startLane;
    const sgChoice = over ? choiceForAgent(over.agent, over.newWorktree, over.only, over.lane) : null;
    const targetAccount = sgChoice ? sgChoice.account : account;
    const targetPlace = sgChoice ? sgChoice.placeNow : placeNow;
    const targetModel = sgChoice ? sgChoice.modelNow : modelNow;
    const targetEffort = sgChoice ? sgChoice.effortNow : effortNow;
    const targetMakesWorktree = sgChoice ? sgChoice.makesWorktree : harnessMakesWorktree;
    if (!where || !targetAccount || !targetPlace) return;
    // The worktree picked below is still coming onto the board: the start row would start in
    // another one meanwhile.
    if (!over && addingNow) {
      setInfo(`Still adding ${addingNow.name}; start again in a moment.`);
      return;
    }
    const newTree = targetMakesWorktree && !where.startsChats && (over ? over.newWorktree : newWorktree);
    // The skill that starts the prompt goes as the account's agent takes it.
    const base = skillNow && harness && !over ? writtenFor(text, [skillNow.skill], harness) : text;
    const written = withAttachments(base, attached);
    // Improve Codealong: the pictures of the windows go after what you wrote.
    const prompt = over?.prompt ?? withShots(written);
    const spec: StartSpec = { laneId: where.id, account: targetAccount, place: targetPlace, prompt, model: targetModel, effort: targetEffort, worktree: newTree, todos: forTodos.map((t) => t.id) };
    if (!over) noteDone({ kind: 'start', target: where.id, newWorktree: newTree }, 'start-row');
    const targetOnModel = sgChoice ? onModelWords(sgChoice.startsOn, sgChoice.effortWord, ownModels(sgChoice.accountInfo, models)) : onModel;
    // With nothing to start on, and nothing of its own opening to write it in, the main window
    // opens a new session for its first prompt, which starts it (newSession.ts); and one on a
    // prompt, already on its way, when the switch says to show it as it starts.
    const shown = !!prompt.trim() && showsStart(targetPlace, showOn, flip);
    if (startsOnFirstSend(targetPlace, !!prompt.trim()) || shown) {
      const does = startDoes({ lane: where, who: `${targetAccount}${targetOnModel}`, onModel: targetOnModel, place: targetPlace, newWorktree: newTree, prompted: true, terminal: terminalName, behind });
      if (shown) keepPrompt(fullPrompt);
      clearPrompt();
      setTodoIds([]);
      setNewWorktree(false);
      setImprove(null);
      setError(null);
      setActiveKey(null);
      void hideQuick();
      void showMain({ kind: 'new', draft: { spec, does, placeLabel: places.find((p) => p.place === targetPlace)?.label ?? targetPlace, harness: harness ?? null, ...(shown && { sent: over?.prompt ?? fullPrompt }) } });
      return;
    }
    const label = startingLabel(targetPlace, targetAccount, targetOnModel, where, newTree, deviceInfo?.name ?? null);
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

  /* ---- Listen now ---- */

  // The meeting in your calendar it would be of, the one it was opened for or one going on or
  // about to start, unless you said it isn't that one; what it asks for, from how it was last
  // asked and what's typed.
  const calendarMeeting = listen ? nowMeeting(meetings, askedMeeting) : null;
  const itsMeeting = calendarMeeting && calendarMeeting.id !== notMeeting ? calendarMeeting : null;
  const listenAsk = nowAsk(listenHow, text);
  const setListenAsk = (next: Partial<ListenAsk>) => setListenHow((how) => ({ role: next.role ?? how.role, mic: next.mic ?? how.mic, system: next.system ?? how.system, start: next.start ?? how.start }));
  // Why it can't listen as it stands: this Mac can't transcribe, or nothing is picked to hear.
  const listenProblem = hearing?.status.why ?? askProblem(listenAsk);
  const listenRow = nowRow({ kept: listenHow, meeting: itsMeeting, where: lane && !lane.startsChats ? `in ${lane.callsign}` : 'as a chat of its own' });

  /** Listen now: Codealong hears the meeting from this moment, with a session started for it,
   *  which shows on the board once it has the listening. The panel closes at once; a start that
   *  fails brings it back with what you wrote and why. Files attached aren't passed on. */
  const listenToIt = () => {
    // Not while a press is heard into the box, as a start isn't.
    if (hearingHere) return;
    if (listenProblem) {
      setError(listenProblem);
      return;
    }
    // The worktree picked below is still coming onto the board: its session would start in
    // another one meanwhile, as a new session's would (`start`).
    if (addingNow) {
      setInfo(`Still adding ${addingNow.name}; listen again in a moment.`);
      return;
    }
    // What the panel was set to, for a start that fails: it may be open for something else by then.
    const kept = { text, attached, project, laneId, device, notMeeting, askedMeeting };
    const id = Date.now();
    const to = opener ?? 'main';
    const named = itsMeeting?.title.trim() || listenAsk.prompt.split('\n')[0].trim();
    clearPrompt();
    setError(null);
    setActiveKey(null);
    void tellOpener(to, { phase: 'begin', id, label: named ? `Starting a session to listen to ${named}…` : 'Starting a session to listen…' });
    void hideQuick();
    listenNow({ meeting: itsMeeting?.id, lane: lane && !lane.startsChats ? lane.id : undefined, ask: listenAsk })
      .then((listening) => tellOpener(to, { phase: 'end', id, key: listening?.session ?? null, message: null }))
      .catch((e: unknown) => {
        void tellOpener(to, { phase: 'end', id, key: null, message: null });
        // Back as it was set to listen, whatever it has been opened for since (the hearing can
        // fail many seconds on): a meeting's name put back into a panel that finds sessions
        // would be looked for, sent to Jev and kept as a prompt's draft.
        reopen(() => {
          // A prompt written there since, not sent, is kept as a draft, as an open keeps one: ↑
          // brings it back once the panel finds and starts again.
          if (!listenWas.current) {
            const typed = fullPromptNow.current;
            const typedFor = todoIdsNow.current;
            const typedOver = improveNow.current?.shots;
            setDraft((d) => nextDraft(typed, typedFor, d, typedOver));
          }
          setListen(true);
          setAsChat(true);
          setImprove(null);
          setTodoIds([]);
          setSendTo(null);
          setRecall(null);
          setProject(kept.project);
          setLaneId(kept.laneId);
          setDevice(kept.device);
          setNotMeeting(kept.notMeeting);
          setAskedMeeting(kept.askedMeeting);
          setText(kept.text);
          setAttached(kept.attached);
          setError(errorText(e));
        });
      });
  };

  /* ---- Talking ---- */

  // The latest press the panel was told of (the engine tells it of the ones whose words are for
  // it), with the open it began in, noted at the first word of it: what a press may do here
  // goes by what the panel was then, not by what it has become since. `over` once it has ended
  // or the panel has dropped it. And the press the panel was opened to hear, once that has begun.
  const pressRef = useRef<{ id: number; opens: number; over: boolean } | null>(null);
  const heardFor = useRef<number | null>(null);
  // What's heard of a press whose words are for this panel, and what came of it. One that
  // failed says why here; one that heard nothing says so, or, when the panel was opened only to
  // hear it and holds nothing, puts the panel away again, so a key pressed by mistake leaves you
  // where you were.
  useEffect(
    () =>
      onTalk((t) => {
        if (t.to !== 'looking') return;
        const latest = pressRef.current;
        // An earlier press, settling after the next began: what it came to says and does
        // nothing over the one being heard. Its words still come (`talked`).
        if (latest && t.id < latest.id) return;
        setTalk(t);
        const going = t.phase === 'listening' || t.phase === 'settling';
        if (!latest || latest.id < t.id) {
          pressRef.current = { id: t.id, opens: opensNow.current, over: false };
          if (openedToHear.current) heardFor.current = t.id;
          openedToHear.current = false;
          // It shows in the box when the box has the keyboard and holds the prompt (not while it
          // finds a session to send to), over what's selected there. While an earlier press's
          // words still show in the box, that's where those are going, and the field's own
          // selection is of what's drawn, not of what it holds: this one goes on from there.
          const el = input.current;
          const before = heardRef.current;
          if (going && shownNow.current && el && document.activeElement === el && !inSendToNow.current) {
            const at = before && before.opens === opensNow.current ? before : { start: el.selectionStart, end: el.selectionEnd };
            setHeard({ id: t.id, opens: opensNow.current, start: at.start, end: at.end });
          }
        }
        if (going) return;
        // Over, with what was said typed (`talked`, before this) or nothing to type.
        const press = pressRef.current;
        if (press) press.over = true;
        if (heardRef.current?.id === t.id) setHeard(null);
        // Put away again only as the open that was for hearing it: opened for something else
        // since, the panel stays.
        const unasked = heardFor.current === t.id && press?.opens === opensNow.current && !textNow.current.trim() && attachedNow.current.length === 0;
        if (heardFor.current === t.id) heardFor.current = null;
        if (!shownNow.current) return;
        if (t.phase === 'failed') setError(t.message ?? 'Codealong couldn’t listen.');
        else if (t.phase === 'nothing' && unasked) void hideQuick();
        else if (t.phase === 'nothing' && t.message) setInfo(t.message);
      }),
    [textNow, attachedNow, shownNow, inSendToNow, opensNow],
  );
  // Put away while a press whose words are for it is going (Escape, a click elsewhere), or
  // folded to its chip, which is what a click elsewhere does to a panel that holds a prompt: the
  // press is dropped, its microphone off and what it heard put nowhere, rather than left
  // listening behind a panel that's gone, or bringing the panel back with it. Whichever field
  // of the panel's it was heard for, and wherever it has got to: the engine drops it by its id.
  useEffect(() => {
    if (shown && !folded) return;
    if (heardRef.current) setHeard(null);
    const press = pressRef.current;
    if (!press || press.over) return;
    press.over = true;
    // Not said when it fails: a press ends by itself as its key comes up, or after three minutes.
    void talkTo(false, press.id).catch(() => {});
  }, [shown, folded]);
  // Shift was down as the key came up (`Talked.start`): what was said starts as ⌘⏎ would, or set
  // to listen, as ⏎ would. Only for the press this box showed and took, in the open it began in:
  // asked a turn after it's typed, of the panel as it is by then.
  const startSpoken = useLatest((opened: number) => {
    if (opened !== opens || inSendTo || hearingHere) return;
    if (listen) listenToIt();
    else start();
  });
  // What was said (`talked`), put where its press was heard: typed over what was selected in the
  // box as the press began, as typing would be, so the box's own code hears it and one Undo takes
  // it out. An earlier press's words, said before the one the box is showing now, are typed
  // there too, and that one goes on after them. A press the box didn't show (another field of
  // the panel's had the keyboard, the panel has been opened for something else since) is put as
  // any window puts it (talk.ts `place`), where the keyboard is now, and starts nothing.
  useEffect(
    () =>
      onTalked((said) => {
        // The engine opens the panel on what was said when it isn't up; one that came anyway
        // goes the same way. Not said when it fails: there's nowhere left to say it.
        if (!shownNow.current) return void showQuick({ prompt: said.text }).catch(() => {});
        const press = pressRef.current;
        if (press && press.id === said.id) press.over = true;
        const at = heardRef.current;
        const el = input.current;
        const inBox = at && el && said.id !== undefined && said.id <= at.id && at.opens === opensNow.current && !inSendToNow.current ? at : null;
        if (!inBox || !el) {
          if (at && at.id === said.id) flushSync(() => setHeard(null));
          return place({ ...said, start: false });
        }
        // The box as it was before the press, for what was said to be typed into.
        flushSync(() => setHeard(null));
        el.focus();
        el.setSelectionRange(inBox.start, inBox.end);
        const typed = document.execCommand('insertText', false, saidAfter(el.value.slice(0, inBox.start), said.text));
        if (said.id !== inBox.id) {
          const caret = el.selectionStart;
          flushSync(() => setHeard({ ...inBox, start: caret, end: caret }));
        } else if (typed && said.start) window.setTimeout(() => startSpoken.current(inBox.opens), 0);
      }),
    [shownNow, inSendToNow, opensNow, startSpoken],
  );
  const talkKeys = shortcutLabel(board?.talkShortcut);

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

  /** Send to: the box finds the session what's typed goes to, while the prompt waits under it as
   *  written. */
  const openSendTo = () => {
    setSendTo('');
    setActiveKey(null);
    setPicker(null);
    refocus();
  };

  /** Back from Send to: the prompt is in the box again, with the caret at its end. */
  const closeSendTo = () => {
    setSendTo(null);
    setActiveKey(null);
    setCaret(textNow.current.length);
    window.requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });
  };

  const suggest = (sg: Suggestion, alt: boolean) => {
    if (sg.kind === 'send') alt ? revealSession(sg.lane, sg.session) : send(sg.session);
    else if (sg.kind === 'open') (alt ? openSession : showSession)(sg.lane, sg.session);
    else if (sg.kind === 'start') start({ lane: sg.lane, newWorktree: sg.newWorktree, agent: sg.agent, only: sg.needs?.accounts });
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
    // Nothing is done with the prompt while a press is still being heard into it.
    if (hearingHere) return;
    const row = rows.find((r) => r.key === key);
    if (row) noteRow(row, alt);
    // Send to has no new session's row to fall back on.
    if (!row) {
      if (!inSendTo) start(undefined, alt);
    } else if (row.kind === 'suggest') suggest(row.s, alt);
    else if (row.kind === 'sendto') openSendTo();
    else if (row.kind === 'todo') (alt ? revealTodo : openTodo)(row);
    else if (row.kind === 'pr') (alt ? openPr : showPr)(row.pr);
    else if (row.kind === 'review') (alt ? revealPr(row.pr) : answerIn(row.pr.key));
    else if (row.kind === 'skill') pickSkill(row);
    else if (row.kind === 'more') {
      // The first of those it adds takes its place.
      setAllFor(query);
      setActiveKey(ranked[FOUND_SESSIONS]?.key ?? null);
    } else if (row.group === 'sendto') suggest(toSessionSuggestion(row.lane, row.session, row.p ?? 0), alt);
    else if (alt) openSession(row.lane, row.session);
    else showSession(row.lane, row.session, row.said && findFor(row.said.text, queryWords(query)));
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
  // The agent menu opened for where it runs (⌘O): its ← → step the place, not the effort.
  const [stepsPlace, setStepsPlace] = useState(false);

  /** ↑ and ↓ through the draft and earlier prompts, in the box `el` (quickKeys.ts): brings back
   *  the one at `at` of `recallable`, or the empty box past the newest of them (-1). */
  const recallAt = (at: number, el: HTMLTextAreaElement) => {
    const next = at < 0 ? '' : recallable[at];
    const shownText = loadPrompt(next);
    setRecall(at < 0 ? null : at);
    const fromDraft = at === 0 && draft && recallable[0] === draft.text ? draft : null;
    setTodoIds(fromDraft?.todos ?? []);
    // A draft written over Improve Codealong's pictures brings them back, and with them the
    // start in Codealong's source; walking on puts back the panel's own pictures and place.
    if (fromDraft?.shots) {
      if (!recalledOver.current) {
        recalledOver.current = { improve, place: improve ? null : { project, laneId, device, asChat } };
        if (!improve) {
          // Where an Improve open starts: the source's worktree, or none until it's found.
          const at = board?.improve?.laneId ?? null;
          setLaneId(at);
          setDevice(at ? shortOf(at) : null);
          if (board?.improve?.project) setProject(board.improve.project);
          setAsChat(false);
          improveFollowed.current = false;
        }
      }
      setImprove({ shots: fromDraft.shots });
    } else if (recalledOver.current) {
      const before = recalledOver.current;
      recalledOver.current = null;
      setImprove(before.improve);
      if (before.place) {
        setProject(before.place.project);
        setLaneId(before.place.laneId);
        setDevice(before.place.device);
        setAsChat(before.place.asChat);
      }
    }
    setActiveKey(null);
    window.requestAnimationFrame(() => el.setSelectionRange(shownText.length, shownText.length));
  };

  // The box's keys (quickKeys.ts).
  const onInputKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) =>
    onQuickBoxKey(e, { hearingHere, listen, inSendTo, rows, keys, active, recallable, recall, attached, setError, listenToIt, activate, closeSendTo, move, pickSkill, answerSession, answerIn, start: (flip) => start(undefined, flip), recallAt });

  // The panel's own keys, wherever the focus is in it (quickKeys.ts). A menu's filter handles its
  // own first. The calmer machine is taken by a function further down.
  useQuickKeys({ askingDev, folded, picker, inSendTo, hearingHere, listen, projects, numbered, active, query, sendable, calmer, repoNow, macs, places, stepsPlace, setPicker, togglePicker, setStepsPlace, refocus, openSendTo, closeSendTo, pickProject, activate, toggleArchived, takeCalmer: (c) => takeCalmer(c), listenToIt, start });

  /* ---- Menus ---- */

  const projectSections: PickerSection[] = projectMenu(projects, counts, board?.chatShortcut);

  // A folder's path typed in the repo menu is looked up as it's typed; an answer goes only
  // with what was typed for it. Another Mac's folders aren't this one's to look in
  // (useFolderLook.ts).
  const { folderTyped, setFolderTyped, typingFolder, looked } = useFolderLook(picker, deviceNow);
  // A folder added comes onto the board in a moment, and is picked from then.
  useEffect(() => {
    if (adding?.laneId && board?.lanes.some((l) => l.id === adding.laneId)) setAdding(null);
  }, [adding, board?.lanes]);

  // Where a pick in the repo menu would start, under the repo's name.
  const startsInRepo = (lanes: Lane[]) => {
    if (newWorktree && makesWorktree) return 'New worktree';
    const l = startLaneIn(lanes, false, isolatesIn);
    return l ? `${worktreeName(l)}${l.origin !== 'main' && l.reuse ? `, ${REUSE_STATE[l.reuse].toLowerCase()}` : ''}` : '';
  };
  // A worktree's name or branch typed where it names no repo lists the worktrees it names, so
  // one you know by name is a pick away from here too.
  const typedWords = filterWords(folderTyped);
  const namesRepo = typedWords.length === 0 || repoList.some((r) => matches(repoOption(r, ''), typedWords));
  const repoMenuSections: PickerSection[] = typingFolder
    ? looked
      ? [{ key: 'folder', options: folderOptions(folderChoices(looked, current, projects.map((p) => p.name)), folderTyped, board?.lanes ?? []) }]
      : []
    : [
        // First, so a long list of repos doesn't hide them: a chat in no project, and a folder
        // the list doesn't have.
        { key: 'elsewhere', options: [...(chatLane ? [chatOption(chatLane.id)] : []), ...(deviceNow ? [] : [anotherFolderOption(folderStart(board?.lanes ?? []))])] },
        ...repoSections(repoList, (r) => startsInRepo(r.lanes)),
        ...(namesRepo ? [] : [{ key: 'worktrees', head: 'Worktrees', options: lanes.map(namedWorktreeOption) }]),
      ];
  const repoValue = lane ? (lane.startsChats ? lane.id : `${REPO}${repoKey(lane)}`) : null;

  // The worktree menu: the repo's, its main checkout first, then where new work starts, the ones
  // that hold other work folded away (quick.ts `repoWorktrees`). Listening makes no worktree.
  const inRepo = repoNow ? repoWorktrees(repoNow.lanes, pinnedWorktrees ?? []) : null;
  const worktreeSections: PickerSection[] = inRepo
    ? worktreeMenu(inRepo, inRepo.from && !listen ? newWorktreeOption(newWorktreeDetail(harnessMakesWorktree, placeNow, harness, worktreeName(inRepo.from), codealongMakesWorktree), newWorktreeTitle(harnessMakesWorktree, placeNow, harness, codealongMakesWorktree), !harnessMakesWorktree) : null)
    : [];

  /** A repo picked in its menu: where new work starts in it, as the place picked has it now or
   *  later (`lane`; New worktree, while it's on, is made from the repo's checkout for it as the
   *  session starts). */
  const pickRepo = (id: string) => {
    const r = id.startsWith(REPO) ? repoList.find((x) => x.key === id.slice(REPO.length)) : undefined;
    if (!r) {
      // A chat, a worktree named by what's typed, or a folder to add: just there, a worktree
      // picked by name being where it starts.
      if (!id.startsWith('folder:')) setNewWorktree(false);
      pickWorktree(id);
      return;
    }
    setPicker(null);
    refocus();
    dropAdding();
    setLaneId(id);
  };

  /** A worktree picked in the repo's menu, or New worktree: on over the worktree picked, which
   *  it starts in once it's off or the account makes none (`startLane`). */
  const pickInRepo = (id: string) => {
    if (id !== NEW_WORKTREE) {
      setNewWorktree(false);
      pickWorktree(id);
      return;
    }
    setPicker(null);
    refocus();
    setNewWorktree(true);
  };

  /** A worktree picked, or a folder typed in the repo menu: added to the project its row says,
   *  then picked once the board has it. */
  const pickWorktree = (id: string) => {
    setPicker(null);
    refocus();
    const turn = ++addTurn.current;
    const project = addsTo(id);
    if (project === undefined || !looked) {
      setAdding(null);
      setLaneId(id);
      return;
    }
    const name = looked.name;
    const from = laneId;
    setAdding({ name, from, laneId: null });
    setError(null);
    // The answer picks the folder only while nothing else has been picked since.
    const ours = () => addTurn.current === turn;
    addFolder(looked.typed, project)
      .then((added) => {
        if (!ours()) return;
        if (laneIdNow.current !== from) {
          setAdding(null);
          return;
        }
        setLaneId(added);
        setAdding({ name, from, laneId: added });
      })
      .catch((e: unknown) => {
        if (!ours()) return;
        setAdding(null);
        setError(errorText(e));
      });
  };

  const agentSections: PickerSection[] = useMemo(
    () => agentMenuSections(agents, accounts, pick, usage, models, now, agentNow, AUTO_STARTS, routingGroups, allLanes),
    [agents, accounts, pick, usage, models, now, agentNow, routingGroups, allLanes],
  );

  const pickAgent = (id: string) => {
    pickAgentLine(id);
    setPicker(null);
    refocus();
  };

  // Where it runs is for this start alone; Make default keeps it as its accounts' own (on this Mac).
  const keepers = choice && placeNow && !deviceNow ? keepPlaceFor(choice, accounts, placeNow, routingGroups, autoGroupIn(optionsForStart, lane?.project)) : [];
  const agentFoot = (
    <AgentFoot
      effort={{ efforts, value: effortShown, onPick: pickEffort, harness, model: effortModel, takesEffort: (accountInfo?.efforts?.length ?? 0) > 0 }}
      place={{
        places,
        value: placeNow,
        onPick: pickPlace,
        stepped: stepsPlace,
        usual: keepers.length > 0 && placeNow ? { kept: keptAt(keepers, accounts, placeNow), whose: keepers.length === 1 ? keepers[0] : 'their', onKeep: () => keepPlace(keepers, placeNow) } : undefined,
        shows: { on: showOn, onFlip: flipShow },
      }}
    />
  );

  const deviceSections: PickerSection[] = useMemo(() => (devices ? machineMenu(devices, macs, board?.lanes ?? [], board?.load, current, now) : []), [devices, macs, board?.lanes, board?.load, current, now]);

  /** Starts on machine `d` (null for this one), in worktree or repo `laneTo` there when given. */
  const toDevice = (d: string | null, laneTo: string | null = null) => {
    if (d !== deviceNow) {
      // Its worktrees and accounts are its own.
      dropAdding();
      setDevice(d);
      setLaneId(laneTo);
      setAgent(null);
      setPlace(null);
      // A Mac that didn't answer is asked again.
      if (d) askAgain(d);
    }
    setPicker(null);
    refocus();
  };
  const pickDevice = (id: string) => toDevice(id === HERE ? null : id);
  /** The calm machine offered, in the checkout of the start's repo there (or its chats), the
   *  start row picked so that ⏎ starts it. */
  const takeCalmer = (c: Calmer) => {
    toDevice(c.on, c.lane.id);
    setActiveKey(COMPOSE);
  };

  /** A suggestion's row: what ⏎ does, and where. */
  const describe = (sg: Suggestion) => {
    const sgChoice = sg.kind === 'start' ? choiceForAgent(sg.agent, sg.newWorktree, sg.needs?.accounts, sg.lane) : null;
    const sgAccount = sgChoice ? sgChoice.account : account;
    const sgPlace = sgChoice ? sgChoice.placeNow : placeNow;
    const sgMakesWorktree = (sgChoice ? sgChoice.makesWorktree : harnessMakesWorktree) && !sg.lane.startsChats;
    const sgOnModel = sgChoice ? onModelWords(sgChoice.startsOn, sgChoice.effortWord, ownModels(sgChoice.accountInfo, models)) : onModel;
    const sgBehind = (sgChoice ? sgChoice.places : places).find((p) => p.place === 'background')?.behind ?? false;
    return suggestionWords(sg, { now, text, account: sgAccount, onModel: sgOnModel, place: sgPlace, makesWorktree: sgMakesWorktree, terminal: terminalName, behind: sgBehind, duplicate });
  };

  /* ---- Render ---- */

  const sections = rowSections(rows, { routed: !!routeFor, searching, inSendTo });
  // Whether a search found nothing, Jev's suggestions and Send to aside.
  const foundNothing = searching && rows.every((r) => r.kind === 'suggest' || r.kind === 'sendto');
  // Send to's rows keep a column for how likely Jev thinks each is, when it weighed any.
  const sendLikely = !!sendList?.rows.some((r) => r.p != null);
  // What every row of the list is drawn from and does (QuickRows.tsx).
  const rowsShown: RowsShown = { active, numbered, now, route, lastRoute, suggested, sendLikely, describe, inWaiting, held: (key) => drag.rowProps(key, null, 0), click: clickRow, menu: rowMenu };
  const nothingWaits = listing && !!attention && attention.counts.top + attention.counts.firstReviews + attention.counts.needs + attention.counts.reviews + attention.counts.merges + attention.counts.unread === 0;
  /** A group's heading count: all there are of it, when the list shows only some. */
  const groupCount = (g: RowGroup, shown: number): number => {
    if (g === 'ready') return readyList?.count ?? shown;
    if (g === 'recent') return recent?.count ?? shown;
    if (g === 'top' || g === 'firstReviews' || g === 'needs' || g === 'reviews' || g === 'merges' || g === 'unread' || g === 'working') return attention?.counts[g] ?? shown;
    if (g === 'found') return foundSessions.length;
    if (g === 'sendto') return sendList?.count ?? shown;
    return shown;
  };
  // Another Mac's accounts come a moment after it's picked, or not at all.
  const activeGroup = isGroup(choice, routingGroups);
  const agentChip = agentChipWords({ auto: isAuto(choice), group: activeGroup?.name ?? null, account, accounts: accounts.length, remote: deviceNow ? { read: !!startOptionsNow, error: remote?.error ?? null } : null, model: modelShown, effortWord });
  // The hint names the shortcut that opened the panel this way: Improve Codealong's when improving,
  // the chat one when it opened on a chat.
  const shortcut = shortcutLabel(listen ? board?.listenShortcut : improve ? board?.improveShortcut : asChat ? board?.chatShortcut : board?.quickShortcut);

  // PR links in the list's rows (PrLink.tsx): a PR, or a session on its card, shows in the main
  // window, as ⏎ on a row does.
  usePrLinks(board?.prs, board?.lanes, {
    report: menuReport,
    showPr: revealPr,
    showSession,
    showTodo: (todo) => {
      void hideQuick();
      void showMain({ kind: 'todo', id: todo.id });
    },
    openSession,
    sessionMenu: (lane, s) => void showContextMenu(sessionMenuSpec(lane, s)).catch((e: unknown) => setError(errorText(e))),
    prMenu: (url, pr) => {
      const spec = pr ? prMenuSpec(pr) : menus.prMenu({ url, number: prRefOf(url)?.number ?? 0 }, { report: menuReport, open: () => openLink(url) });
      void showContextMenu(spec).catch((e: unknown) => setError(errorText(e)));
    },
    openLink,
  }, board, deviceLanes);

  // Its rows say a worktree's or a PR's project where it isn't the one picked (ProjectSaid.tsx).
  const page = (
    <div
      ref={root}
      className={cx('ca-quick-root', folded && 'is-folded')}
      onMouseDown={(e) => {
        // The shadow around the card is part of the window; a click there is a click outside, which
        // hides it, but for a panel holding a prompt: that one folds once another app has the
        // keyboard, and folded here it would keep it. Round the chip it opens the panel again, as
        // the chip does.
        if (e.target !== e.currentTarget) return;
        if (folded) void showQuick({ keep: true });
        else if (!holdsPrompt) void hideQuick();
      }}
    >
      {drag.ghost}
      {folded && <QuickFold chip={foldChip} words={foldedWords(text, attached.length)} shortcut={shortcut} />}
      <div className="ca-quick" role="dialog" aria-label="Codealong" style={{ height: cardHeight(room, tall) }}>
        <div className="ca-quick-head">
        <div className="ca-quick-input">
          <div className="ca-quick-pick">
            <button type="button" data-picker-trigger className="ca-quick-project" title={keysIn('Project (⌘P)')} aria-haspopup="dialog" aria-expanded={picker === 'project'} onClick={() => togglePicker('project')}>
              <ProjectIcon src={projectIcons.get(current)} size={16} fallback={<Mark size={16} status={logoStatus} />} />
              <span>{projectLabel(current)}</span>
              <Icon name="chevron-down" size={12} />
            </button>
            {picker === 'project' && <Picker sections={projectSections} value={current} placeholder="Project" onPick={pickProject} onClose={() => { setPicker(null); refocus(); }} />}
          </div>
          <textarea
            ref={input}
            value={inSendTo ? sendTo : boxText}
            // Not typed into while a press is heard into it, from its first word to its last:
            // where the words go is where the box was when it began, and what shows meanwhile
            // is drawn, not written.
            readOnly={!!hearingHere}
            rows={1}
            spellCheck={!inSendTo}
            aria-label={inSendTo ? SEND_TO_PLACEHOLDER : listen ? 'What the meeting is' : 'Search or prompt'}
            placeholder={inSendTo ? SEND_TO_PLACEHOLDER : talking ? 'Listening…' : listen ? nowPlaceholder(listenHow.role) : improve ? IMPROVE_PLACEHOLDER : draft ? `${QUICK_PLACEHOLDER} ↑ brings back your draft` : history.length ? `${QUICK_PLACEHOLDER} ↑ for earlier prompts` : QUICK_PLACEHOLDER}
            onChange={(e) => {
              setActiveKey(null);
              if (inSendTo) {
                setSendTo(e.target.value);
                return;
              }
              setText(e.target.value);
              setCaret(e.target.selectionStart);
              setRecall(null);
            }}
            onSelect={(e) => !inSendTo && setCaret(e.currentTarget.selectionStart)}
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
          {shortcut && !text && !inSendTo && !talking && (
            <span className="ca-quick-hotkey" title="Opens this panel from any app. Change it in Settings.">
              {shortcut}
            </span>
          )}
          {/* Talking into the box (dictation.rs), while voice is on: a click begins it and another
              ends it, as the key held to talk going down and coming up. */}
          {board?.experimental?.voice && !inSendTo && (
            <button
              type="button"
              className={cx('ca-quick-mic', talking && 'is-on')}
              aria-pressed={talking}
              aria-label={talking ? 'Stop listening' : 'Talk'}
              title={keysIn(
                talking
                  ? 'Listening: click, or let go of the key, and what you said is typed here. With Shift held as you do, it starts a session with it at once.'
                  : `Talk, and what you say is typed here: click, and click again when you’re done${talkKeys ? `, or hold ${talkKeys} from any app` : ''}. With Shift held as you stop, it starts a session with it at once.`,
              )}
              // The keyboard stays in the box, where the words go.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => void talkTo(!talking).catch((e: unknown) => setError(errorText(e)))}
            >
              {talking && talk ? <Level level={talk.level} /> : <Icon name="microphone" size={15} />}
            </button>
          )}
        </div>
        {inSendTo && (
          // The prompt Send to sends, as written; a click puts it back in the box, as Escape does.
          <button type="button" className="ca-quick-sending" title="Edit the prompt (Esc)" onMouseDown={(e) => e.preventDefault()} onClick={closeSendTo}>
            <Icon name="send" size={12} />
            <span className="ca-quick-sending-text">{text.trim()}</span>
            <Kbd keys={['esc']} />
          </button>
        )}
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
            scrolledByYou.current = true;
          }}
          onMouseLeave={() => {
            aim.current = aimedAtNothing(aim.current);
          }}
        >
          <div
            ref={scroller}
            className="ca-quick-scroll"
            // A press on the list itself, not on a row in it, is on its scroll bar, where macOS
            // shows one: dragging it scrolls as the wheel does.
            onPointerDown={(e) => {
              if (e.target === e.currentTarget) scrolledByYou.current = true;
            }}
          >
            {listen ? (
              <ListenAsked meeting={calendarMeeting} its={!!itsMeeting} setNotMeeting={setNotMeeting} now={now} ask={listenAsk} set={setListenAsk} />
            ) : !board || board.loading ? (
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
                {typing && rows.length === 0 && <p className="ca-quick-note">{listedSkills ? (repos.length === 0 ? 'No skill of your accounts starts like that, and no repo here to read others from.' : `No skill of ${current === ALL_PROJECTS || chatting ? 'these repos' : `${current}’s repos`} or your accounts starts like that.`) : 'Reading skills…'}</p>}
                {sections.map((s) => (
                  <div key={s.group} ref={s.group === 'suggested' ? suggestedGroup : undefined} className="ca-quick-group" role="group" aria-label={GROUP_WORD[s.group]}>
                    <GroupLabel s={s} count={groupCount(s.group, s.rows.length)} working={routing && lastSuggested.length > 0} withArchived={withArchived} archivedFound={archivedFound} onArchived={() => {
                      toggleArchived();
                      refocus();
                    }} />
                    {/* Jev's suggestions, or what stands in for them, come before Send to's row. */}
                    {s.group === 'suggested' && !s.rows.some((r) => r.kind === 'suggest') && <SuggestedStandIn last={lastSuggested} describe={describe} routing={routing} routeProblem={routeProblem} startRowSuggested={!!startRowSuggested} asSearch={asSearch} startLeads={startLeads} />}
                    {s.rows.map((r) => (
                      <QuickRow key={r.key} r={r} list={rowsShown} />
                    ))}
                    <GroupNotes s={s} foundNothing={foundNothing} sendToIn={sendToIn} archivedFound={archivedFound} withArchived={withArchived} sendList={sendList} building={!!textFound?.building} />
                  </div>
                ))}
              </>
            )}
          </div>
          {listen && <ListenRow problem={listenProblem} title={listenRow.title} why={listenProblem ?? [macSays(hearing?.status, listenAsk), listenRow.why, attached.length > 0 && 'Files attached aren’t passed on.'].filter(Boolean).join(' ')} onClick={listenToIt} />}
          {!inSendTo && !listen && (
            <StartRow active={active === COMPOSE} ready={ready} title={startTitle(forTodos.length, lane ? startTitleWords(lane, inNewWorktree) : null)} where={lane ? startWhereWords(lane) : null} suggested={startRowSuggested} from={route?.model ?? 'Jev'} onClick={(e) => clickRow(COMPOSE, e)}>
              {/* The warning leads: the row's line is cut short when it's long. */}
              {primaryRunningWords(held, forTodos.length)}
              {skillNow && (skillNow.lane ? `Runs /${skillNow.skill.name} from ${skillNow.lane.repo}. ` : `Runs /${skillNow.skill.name}. `)}
              {happens}
              {skillLeftOut && ` ${skillLeftOut}`}
              {alreadyOn}
              {attached.length + shots.length > 0 && attachmentsNote(attached.length + shots.length)}
              {newToCodex && ' Codex adds this folder as a project if it isn’t one yet.'}
              {deviceNow && forTodos.length > 0 && todosStayNote(forTodos.length)}
            </StartRow>
          )}
        </div>
        {todo && <TodoStrip todo={todo} forTodos={forTodos} blocked={blocked} waitsOn={waitsOn} held={held} marked={marked} showSession={showSession} unlink={() => setTodoIds([])} />}
        <QuickNotes error={error} info={info} improveNote={improveNote} addCodealongProject={addCodealongProject} calmer={calmer} startsOnLoad={startsOnLoad} startsOnName={startsOnName} takeCalmer={takeCalmer} />
        {!inSendTo && (
          <div className="ca-quick-bar">
            {macs.length > 0 && !listen && (
              <div className="ca-quick-pick">
                <button type="button" data-picker-trigger className="ca-quick-chip" title={keysIn(deviceInfo ? `On ${deviceInfo.name}\nMachine (⌘D)` : 'Machine (⌘D)')} aria-haspopup="dialog" aria-expanded={picker === 'device'} onClick={() => togglePicker('device')}>
                  {deviceInfo ? <i className={`ca-labeldot is-${deviceInfo.color}`} aria-hidden="true" /> : <Icon name="device-desktop" size={14} />}
                  <span>{deviceInfo ? deviceLabel(deviceInfo.name) : 'This machine'}</span>
                  <Icon name="chevron-down" size={12} />
                </button>
                {picker === 'device' && <Picker sections={deviceSections} value={deviceNow ?? HERE} placeholder="Machine" onPick={pickDevice} onClose={() => { setPicker(null); refocus(); }} />}
              </div>
            )}
            <div className="ca-quick-pick">
              <button type="button" data-picker-trigger className="ca-quick-chip" title={keysIn(addingNow ? `Adding ${addingNow.name}…` : lane ? `${lane.startsChats ? 'A general chat, in a folder of its own: no repo, no project' : `${lane.repo}, in ${lane.project}`}\nRepo (⌘T)` : 'Repo (⌘T)')} disabled={lanes.length === 0 && !chatLane} aria-haspopup="dialog" aria-expanded={picker === 'repo'} onClick={() => togglePicker('repo')}>
                <Icon name={addingNow ? 'folder-plus' : lane?.startsChats ? 'message-circle' : 'folder'} size={14} />
                <span>{addingNow ? addingNow.name : lane ? (lane.startsChats ? GENERAL_CHAT : lane.repo) : 'No repo'}</span>
                <Icon name="chevron-down" size={12} />
              </button>
              {picker === 'repo' && (
                <Picker
                  sections={repoMenuSections}
                  value={repoValue}
                  placeholder={deviceNow ? 'Repo, or a worktree by name' : 'Repo, a worktree by name, or a folder’s path'}
                  empty={typingFolder ? 'Looking for it…' : undefined}
                  onPick={pickRepo}
                  onFilter={setFolderTyped}
                  onClose={() => {
                    setPicker(null);
                    refocus();
                  }}
                />
              )}
            </div>
            {lane && repoNow && !addingNow && (
              <div className="ca-quick-pick">
                <button type="button" data-picker-trigger className="ca-quick-chip" title={keysIn(`${inNewWorktree ? newWorktreeTitle(true, placeNow, harness, codealongMakesWorktree) : tildePath(lane.path)}\nWorktree (⌘B)`)} aria-haspopup="dialog" aria-expanded={picker === 'worktree'} onClick={() => togglePicker('worktree')}>
                  <Icon name={inNewWorktree ? 'plus' : 'branch'} size={14} />
                  <span>{inNewWorktree ? 'New worktree' : worktreeName(lane)}</span>
                  <Icon name="chevron-down" size={12} />
                </button>
                {picker === 'worktree' && (
                  <Picker
                    sections={worktreeSections}
                    value={inNewWorktree ? NEW_WORKTREE : lane.id}
                    placeholder="Worktree or branch"
                    onPick={pickInRepo}
                    onClose={() => {
                      setPicker(null);
                      refocus();
                    }}
                  />
                )}
              </div>
            )}
            {listen && <span className="ca-quick-listen-on">Its session starts on the account Settings › Experimental names for meetings.</span>}
            <div className="ca-quick-pick ca-quick-agent" hidden={listen}>
              <button type="button" data-picker-trigger className="ca-quick-chip" title={keysIn('Account, model, effort (⌘L) and where it runs (⌘O)')} disabled={accounts.length === 0} aria-haspopup="dialog" aria-expanded={picker === 'agent'} onClick={() => { setStepsPlace(false); togglePicker('agent'); }}>
                <Icon name={autoFor ? 'sparkles' : 'user'} size={14} />
                <span>{agentChip.label}</span>
                {agentChip.detail && <span className="ca-quick-chip-detail">{agentChip.detail}</span>}
                {placeShown && places.length > 1 && (
                  <span className="ca-quick-chip-place">
                    <Icon name={placeIcon(placeShown.place)} size={13} />
                    {placeShown.label}
                  </span>
                )}
                <Icon name="chevron-down" size={12} />
              </button>
              {picker === 'agent' && <Picker className="is-wide" sections={agentSections} value={agentNow} placeholder="Account or model" onPick={pickAgent} onClose={() => { setPicker(null); refocus(); }} foot={agentFoot} onStep={stepsPlace && places.length > 1 ? (delta) => { const p = placeStep(places, placeNow, delta); if (p) pickPlace(p); } : efforts.length > 0 ? stepEffort : undefined} />}
            </div>
          </div>
        )}
        {askingDev && <DevSetupDialog account={account} source={devSource && tildePath(devSource)} onSetUp={setUpDev} onClose={closeDevAsk} />}
        {askingHelpers.dialog}
      </div>
    </div>
  );
  return (
    <ProjectShown.Provider value={projectShownIn(current)}>
      <ProjectIcons.Provider value={projectIcons}>{page}</ProjectIcons.Provider>
    </ProjectShown.Provider>
  );
}

/** The panel folded while it holds a prompt you haven't sent and another window has the keyboard
 *  (quick.rs `put_aside`): the prompt's first line, where the card's top was, over every app. A
 *  click opens the panel again as you left it, as its shortcut does; × puts it away, and ↑ in the
 *  box brings the prompt back later, as any draft. */
function QuickFold({ chip, words, shortcut }: { chip: RefObject<HTMLDivElement | null>; words: string; shortcut: string | null }) {
  return (
    <div ref={chip} className="ca-quick-fold">
      <button type="button" className="ca-quick-fold-open" title="Go on with your prompt" onClick={() => void showQuick({ keep: true })}>
        <Icon name="pencil" size={14} />
        <span className="ca-quick-fold-words">{words}</span>
        {shortcut && <span className="ca-quick-hotkey">{shortcut}</span>}
      </button>
      <Button variant="ghost" size="sm" icon="x" aria-label="Put away" title="Put away: ↑ in the panel brings it back (esc)" onClick={() => void hideQuick()} />
    </div>
  );
}
