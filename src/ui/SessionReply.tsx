// Answering a session from Codealong: a box that sends it a message where it runs (send.rs), and,
// when it waits on you to allow a tool call, that call above the box with Allow and Deny, if its
// harness can be answered from here (harness.rs: OpenCode through its service, Claude Code by a
// keypress in its Ghostty, iTerm2 or tmux tab, once its own record says the prompt is up). Otherwise it
// offers to open the session to answer there; questions it asks still show, to read before you go.
// The session panel, the floating bar's peek and the waiting window all end with it.
//
// The box stays while the session waits on you, so a session that starts waiting as you type
// doesn't take what you wrote away. While it waits on a call Codealong can answer, sending what
// you wrote denies the call and tells it that instead, as Claude Code's own "No, and tell Claude
// what to do differently" does (actions.rs `answer_call`); otherwise you can send it once the
// session is no longer waiting. Nothing is ever typed at a prompt that's up, where a Return would
// answer it. What you wrote is kept per session, for every window (drafts.ts), until you send it.
//
// Sending clears the box at once and the thread shows the message on its way (ahead.ts), so you
// can go on writing the next; messages to one session go in the order you sent them. One that
// doesn't go is back in the box, with why.
//
// A session that needs you only because its last reply asks you something sits at its prompt, so
// what you type is the answer.
//
// Where there's a next session waiting on you (the Sessions view's panel), ⌘⏎ sends and goes on to
// it. While the session waits on a call that can be answered here, ⌘⏎ answers it and goes on: with
// words in the box it denies the call and sends them instead, as ⏎ does, and with none it allows it.
//
// The box is laid out as chat apps' are: the send button sits inside it, at its corner, and the
// placeholder says how a message gets there (typed into its Ghostty tab, a message from
// Codealong, its app: send.rs `way_in`). What else it has to say (how a send went, the keys that
// aren't the usual ones) goes on a line below it, there only while there's something to say.
//
// A few of Codealong's own slash commands work in the box (slash.ts): `/fork [prompt]` forks the
// session into a copy that goes on in the background, on the prompt when there is one, and you
// stay where you are; `/side <question>` (or `/btw`) asks a throwaway copy of it, whose answer
// shows above the box (SideStrip.tsx) while the session carries on, unaware, and `/side` alone
// shows the side conversations kept from before. Any other `/…` goes to the session as typed.
// While a name is typed after a slash, at the start or after a space, wherever the caret is, a
// list above the box has those commands and the session's skills (skills.ts: its worktree's,
// then its account's): ↑ ↓ choose, ⇥ or ⏎ finishes the name, Escape puts the list away. Each
// skill named goes as its harness takes it (`$name` for Codex).
//
// While a side conversation shows above it (the open one, the list, one being read, or a question
// whose copy is starting), the box is the side conversation's, as Codex's `/side` takes its box
// until you go back: ⏎ asks the copy (a new one when none is open), Escape leaves the session's
// turn alone, and the placeholder and the line under the box say where it goes. Closing what
// shows gives the box back to the session. What shows is the same in every window showing the
// session (side.ts), and the box holds a draft of its own while it's the side conversation's
// (drafts.ts `sideDraft`), so what's typed for the copy never reaches the session from any of them.
// Popped out into a window of its own (`SideReply`), the side conversation has a box there that
// only ever asks it, and the session's boxes are the session's again; `/side` in one of them
// still asks it, and brings its window forward.
//
// While its turn runs, Escape in the box stops it where Codealong can reach the session (the
// engine's `stoppable`), as Escape does in Claude Code and Codex, and so does the button while
// the box is empty, whose placeholder says so (⌘. too, from anywhere in the window). It sends what the harness's own Stop
// sends, or presses Escape in its terminal. The session carries on, waiting for you.
//
// Replies another session suggested for this one (`codealong session suggest`) show above the box
// (SuggestedReplies.tsx), the box's own: a click sends one as ⏎ sends what's typed, leaving what's
// in the box where it is, and ⌥-click puts it in the box to change first. They're gone once you've
// sent the session anything, here or anywhere, and while the box is a side conversation's.
//
// While its turn runs, ⏎ sends as the session's own box would: Claude Code takes the message in
// once the step it's on ends (a command it's running included), the Codex app and OpenCode after
// the turn. Where the session can take it sooner (the engine's `steerable`), the bolt beside Send
// and ⌃⏎, Claude Code's own key for it, send it now, into the turn it's taking: Claude Code's send
// now, which moves a running command to the background so the agent reads the message at once,
// and Codex's and OpenCode's steer.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent } from 'react';
import { sendAhead, sending, useAhead } from '../ahead';
import { shortOf } from '../devices';
import { dismissSuggestions, forkSession, sessionAnswer, sessionCancel, sessionPending, sessionWay, sideAsk, sideWindow, switchSessionModel } from '../api';
import { filesToAttach, keepFiles } from '../attachInput';
import { onDraftChange, readDraft, sideDraft, writeDraft } from '../drafts';
import { sessionPlace } from '../harness';
import { stoppable } from '../lanes';
import { notAsked, showSides, startingSide, useAside, usePopped } from '../side';
import { COMMANDS, slashCommand, slashMatches } from '../slash';
import { completed, matching, mentionAt, mentionsIn, sessionSkills, skillNamed, writtenFor } from '../skills';
import { rememberResumed, resumeChoice, resumedIn, resumeIn, resumeWords, type ResumePicks } from '../start';
import { accountOf, useStartAccounts } from '../startAccounts';
import { sessionSkillsFor, useSkills } from '../useSkills';
import { attachmentsIn, insertAttachments, withoutAttachment } from '../todos';
import type { Answers, Pending, Question, Sent, Session, StartPlace, Way } from '../types';
import { CodeBlock } from './CopyButton';
import { AttachmentStrip } from './DescriptionEditor';
import { Markdown } from './Markdown';
import { BoxText, Button, Chip, cx, Icon, Rich, Seg } from './primitives';
import { nextQuestion, QuestionList } from './QuestionList';
import { offersResume, ResumeModel, ResumePlace } from './ResumeBar';
import { SessionModel } from './SessionModel';
import { SESSION_SIDES, SideStrip, type SideWindowCan } from './SideStrip';
import { SlashList, type SlashOption } from './SlashList';
import { SuggestedReplies, type PickHow } from './SuggestedReplies';
import { useLatest } from '../latest';
import { errorText } from '../report';

/** Nothing picked for a resume: it goes on where it went on last, on the account's own model. */
const NO_PICKS: ResumePicks = { agent: null, effort: null, place: null };

/** The box grows to this many lines, then scrolls. */
const LINES = 8;
/** How long a Stop waits for the turn to end before saying it hasn't. */
const STOP_WAIT_MS = 8000;

/** Whether typing reaches the session now: anything but one waiting on you to answer a prompt
 *  (a permission, or a turn stuck on a command). */
export function takesReply(s: Session): boolean {
  return s.status !== 'needs' || !!s.asks;
}

/** What the box asks for, and how a message gets there (`way`, once the engine has said): typed
 *  into its terminal, pasted into its background job, a message from Codealong, or its app; or,
 *  where it can't reach the session itself, what happens instead. A session that isn't running
 *  goes on where its place chip says (`resuming`), else in a terminal. */
export function replyPlaceholder(s: Session, terminal: string, way?: Way | null, resuming?: { place: StartPlace; behind: boolean } | null): string {
  const to = s.asks ? `Answer ${s.account}` : `Send to ${s.account}`;
  if (s.hosted) return s.status === 'working' || s.status === 'needs' ? `${to}: it takes this after its current turn` : to;
  const working = s.status === 'working' ? ': it takes this in as it works' : '';
  switch (way?.kind) {
    case 'typed':
      return `${to} via ${way.app}${working}`;
    case 'background':
      return `${to} via its background job${working}`;
    case 'message':
      return `${to} as a message from Codealong${working}`;
    case 'codex-app':
      return `${to} via the Codex app${working}`;
    case 'opencode':
      return `${to} via OpenCode${working}`;
    case 'resume':
      return `${to}: it isn’t running, so ${resumeWords(resuming?.place ?? 'terminal', way.app, resuming?.behind)}`;
    case 'claude-app':
      return `${to}: it isn’t running, so the Claude app opens it with this on the clipboard`;
    case 'cursor':
      return `${to}: Cursor opens with this on the clipboard`;
  }
  if (s.harness === 'antigravity' && s.surface === 'app') return `${to}: Antigravity opens with this on the clipboard`;
  if (s.harness === 'cursor' && s.surface === 'app') return `${to}: Cursor opens with this on the clipboard`;
  if (!s.live && s.harness === 'claude') return s.surface === 'app' ? `${to}: it isn’t running, so the Claude app opens it with this on the clipboard` : `${to}: it isn’t running, so this resumes it in ${terminal}`;
  if (!s.live && (s.harness === 'codex' || s.harness === 'antigravity' || s.harness === 'cursor') && s.surface === 'cli') return `${to}: it isn’t running, so this resumes it in ${terminal}`;
  if (s.status === 'working') return `${to}: it takes this in as it works`;
  return to;
}

interface ComposerProps {
  session: Session;
  terminal: string;
  /** Puts the cursor in the box as it shows. */
  autoFocus?: boolean;
  /** The box is the side conversation's whatever shows: it's in the side conversation's own
   *  window (`SideReply`). */
  onlyAside?: boolean;
  /** The session waits on you (Approval above says for what): what you write waits too. */
  waiting?: boolean;
  /** As a message goes on its way: the box is clear and the thread shows it. */
  onSending?: () => void;
  /** After a message went through, with what happened. */
  onSent?: (sent: Sent) => void;
  /** A message didn't go and no box on its session was showing to say so. It's back in the
   *  session's draft. */
  onFailed?: (why: string) => void;
  /** ⌘⏎ sends, then goes on to the next session that waits on you, saying what was done. */
  onNext?: (done: string) => void;
  /** ⌘⏎ with nothing written allows the call the session waits on, then goes on where there's a
   *  next. */
  onAllow?: () => void;
  /** ⌘⌫ in an empty box denies it, and stays: what to do instead goes in the box. */
  onDeny?: () => void;
  /** Sending while it waits denies the call and tells it what you wrote instead, then with `next`
   *  goes on as ⌘⏎ does. Resolves with why it didn't go, or null once it did. */
  onDenySaying?: (words: string, next: boolean) => Promise<string | null>;
}

type Note = { text: string; error: boolean };

/** How a message to each session would go, as the engine last said, so a box that shows again
 *  says it at once. */
const ways = new Map<string, Way | null>();

/** How what's typed gets to the session, asked again as the session starts or stops running
 *  somewhere: for its box to say, and for Run to know whether a `!` command goes in
 *  (RunButton.tsx `runsInSession`). Null until the engine says, and for one Codealong runs. */
export function useWay(s: Pick<Session, 'key' | 'live' | 'pid' | 'surface' | 'attachCommand'>): Way | null {
  const [way, setWay] = useState<Way | null>(() => ways.get(s.key) ?? null);
  useEffect(() => {
    let live = true;
    sessionWay(s.key).then(
      (w) => {
        ways.set(s.key, w);
        if (live) setWay(w);
      },
      // Said nothing: the box says it as it would before asking.
      () => {},
    );
    return () => {
      live = false;
    };
  }, [s.key, s.live, s.pid, s.surface, s.attachCommand]);
  return way;
}

/** How a send went, by session, for the box showing on it now or, with none, the next. */
const landed = new Map<string, Note>();
/** The boxes showing, by session, each hearing how its session's sends go. */
const hearing = new Map<string, Set<(note: Note) => void>>();

/** Tells the boxes showing on a session how a send went, or keeps it for the next one. Says
 *  whether a box heard. */
function land(key: string, note: Note): boolean {
  const boxes = hearing.get(key);
  if (!boxes || boxes.size === 0) {
    landed.set(key, note);
    return false;
  }
  landed.delete(key);
  for (const hear of boxes) hear(note);
  return true;
}

function Composer({ session: s, terminal, autoFocus, onlyAside = false, waiting, onSending, onSent, onFailed, onNext, onAllow, onDeny, onDenySaying }: ComposerProps) {
  // A side conversation shows above the box, in this window and every other, so the box is its.
  const aside = useAside(s.key) || onlyAside;
  // Out in a window of its own, where what's asked of it here shows.
  const popped = usePopped(s.key) && !onlyAside;
  // The box holds the draft for where it sends: the session's, or its side conversation's, kept
  // apart so what's typed for one never goes to the other.
  const draft = aside ? sideDraft(s.key) : s.key;
  const draftNow = useLatest(draft);
  const [text, setTextNow] = useState(() => readDraft(draft));
  const [drafting, setDrafting] = useState(draft);
  if (drafting !== draft) {
    setDrafting(draft);
    setTextNow(readDraft(draft));
  }
  // A message to this session on its way, from this box or any other.
  const going = sending(useAhead(), s.key);
  const [note, setNote] = useState<Note | null>(() => landed.get(s.key) ?? null);
  const box = useRef<HTMLTextAreaElement>(null);
  const latest = useRef(text);
  latest.current = text;
  const attached = attachmentsIn(text);
  const way = useWay(s);
  // The suggestions (by when they were made) that what this box sent answered, or that were
  // dismissed here: they go at once, the board saying so a moment later.
  const [answered, setAnswered] = useState<number | null>(null);
  const suggestionList = useRef<HTMLDivElement>(null);

  // Where a session on this Mac that isn't running goes on, and on what (ResumeBar.tsx), when
  // its account offers a choice; without one, a send resumes it in a terminal. The place and the
  // model are chips beside Send, where a running one's model is.
  const here = !shortOf(s.key);
  const resumable = way?.kind === 'resume' && here;
  const account = accountOf(useStartAccounts(here && (resumable || s.live)), s);
  const [resumePicks, setResumePicks] = useState<ResumePicks>(NO_PICKS);
  useEffect(() => setResumePicks(NO_PICKS), [s.key]);
  const resumeChosen = resumable && account ? resumeChoice(account, [], resumePicks, resumedIn(account.name)) : null;
  const resuming = offersResume(resumeChosen) ? resumeChosen : null;
  const resumeHow = resuming ? resumeIn(resuming) : null;
  const resumeWhere = resumeHow && { place: resumeHow.place, behind: !!resuming?.places.find((p) => p.place === resumeHow.place)?.behind };
  // A running one says what model it's on, beside Send, and where Codealong can switch it
  // (SessionModel.tsx): one it runs, or one whose terminal it types into, by its harness's command.
  const switches = here && s.live && !!account && (account.models?.length ?? 0) > 0 && (!!s.hosted || (way?.kind === 'typed' && !!account.switchesModel));
  const saysModel = !resuming && s.live && (switches || !!s.model);
  const [switching, setSwitching] = useState(false);
  const switchModel = (model: string) => {
    setSwitching(true);
    setNote(null);
    switchSessionModel(s.key, model)
      .then(
        (said) => setNote({ text: said, error: false }),
        (e: unknown) => setNote({ text: errorText(e), error: true }),
      )
      .finally(() => setSwitching(false));
  };

  const setText = (t: string) => {
    setTextNow(t);
    latest.current = t;
    writeDraft(draftNow.current, t);
  };

  // Another window wrote to this draft too, or sent what was written, or a prompt the session
  // took back was put back here.
  useEffect(
    () =>
      onDraftChange(draft, (t, focus) => {
        setTextNow(t);
        latest.current = t;
        const el = box.current;
        if (!focus || !el) return;
        el.focus();
        window.requestAnimationFrame(() => el.setSelectionRange(t.length, t.length));
      }),
    [draft],
  );

  // How this session's sends go, whichever box sent them. A message that didn't go is back in the
  // draft, so the box takes the draft again.
  useEffect(() => {
    // Said here, so no later box says it again. (An effect, not the initializer: StrictMode
    // mounts a box twice in development and keeps the second.)
    landed.delete(s.key);
    const hear = (n: Note) => {
      setNote(n);
      if (!n.error) return;
      const t = readDraft(draftNow.current);
      setTextNow(t);
      latest.current = t;
      box.current?.focus();
    };
    const boxes = hearing.get(s.key) ?? new Set<(note: Note) => void>();
    boxes.add(hear);
    hearing.set(s.key, boxes);
    return () => {
      boxes.delete(hear);
      if (boxes.size === 0) hearing.delete(s.key);
    };
  }, [s.key, draftNow]);

  useLayoutEffect(() => {
    const el = box.current;
    if (!autoFocus || !el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [autoFocus]);

  // A session whose turn is running can be stopped from here, where Codealong can reach it. The
  // button says Stopping until the turn ends, and says so if it hasn't a while later.
  const canStop = stoppable(s);
  const [stopping, setStopping] = useState(false);
  useEffect(() => {
    if (!stopping) return;
    if (!canStop) {
      setStopping(false);
      return;
    }
    const t = window.setTimeout(() => {
      setStopping(false);
      setNote({ text: 'It still says it’s working. Open it to stop it there.', error: true });
    }, STOP_WAIT_MS);
    return () => window.clearTimeout(t);
  }, [stopping, canStop]);
  const stop = () => {
    if (stopping) return;
    setStopping(true);
    setNote(null);
    sessionCancel(s.key).then(
      (said) => setNote({ text: said, error: false }),
      (e: unknown) => {
        setStopping(false);
        setNote({ text: errorText(e), error: true });
      },
    );
  };
  const stopNow = useRef(stop);
  stopNow.current = stop;
  // ⌘. stops it too, as it stops things in the Mac's own apps, wherever the focus is in the window.
  useEffect(() => {
    if (!canStop) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '.' || !e.metaKey || e.shiftKey || e.altKey || e.ctrlKey || e.defaultPrevented) return;
      e.preventDefault();
      stopNow.current();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [canStop]);

  // Where the caret is, for the `/name` being typed there; moved as the text is typed, the
  // caret moved, or a name finished.
  const [caret, setCaret] = useState(0);
  const mention = mentionAt(text, Math.min(caret, text.length));
  // The session's skills, read once a slash is typed (its worktree's and its account's).
  const slash = mentionsIn(text).length > 0;
  const { folders: skillFolders, accounts: skillAccounts } = sessionSkillsFor(s);
  const skills = sessionSkills(useSkills(skillFolders, skillAccounts, slash), s.harness);

  /** Sends what's in the box, or a suggested reply that was `chosen` (the box keeping what's in
   *  it), into the turn it's taking at once with `now`; says whether there was anything to send. A
   *  deny it sends goes on to the next with `next` once it's in. */
  const send = (now = false, next = false, chosen?: string): boolean => {
    // A skill's name goes as the session's harness takes it.
    const t = chosen?.trim() ?? writtenFor(text.trim(), skills, s.harness);
    if (!t) return false;
    if (waiting) return denySaying(t, next, chosen !== undefined);
    setNote(null);
    if (chosen === undefined) setText('');
    // What happens next is told to whichever box is showing then: this one may be gone by then
    // (the peek closes as it sends).
    const key = s.key;
    // What's sent answers what was suggested, which goes from here at once rather than when the
    // board says.
    const suggestedAt = s.suggestions?.at;
    if (resumeHow) rememberResumed(s.account, resumeHow.place);
    sendAhead(s, t, { now, resume: resumeHow ?? undefined }).then(
      (sent) => {
        if (suggestedAt !== undefined && !sent?.opened) setAnswered(suggestedAt);
        land(key, { text: sent?.message ?? 'Sent.', error: false });
        onSent?.(sent);
      },
      (e: unknown) => {
        const why = errorText(e);
        if (!land(key, { text: why, error: true })) onFailed?.(why);
      },
    );
    onSending?.();
    return true;
  };

  /** While it waits on a call that can be answered here: denies it with `t` as what to do
   *  instead. The box clears at once, and has what you wrote back, with why, if it doesn't go. A
   *  suggested reply that was `chosen` leaves the box as it is, and is put in it if it doesn't go. */
  const denySaying = (t: string, next: boolean, chosen = false): boolean => {
    if (!onDenySaying) return false;
    const key = s.key;
    const typed = chosen ? t : text;
    const suggestedAt = s.suggestions?.at;
    setNote(null);
    if (!chosen) setText('');
    void onDenySaying(t, next).then((why) => {
      if (why == null) {
        if (suggestedAt !== undefined) setAnswered(suggestedAt);
        return;
      }
      writeDraft(key, [typed, readDraft(key)].filter((x) => x.trim()).join('\n\n'));
      if (!land(key, { text: why, error: true })) onFailed?.(why);
    });
    return true;
  };

  // `/fork`, and what follows it for the copy. The box clears at once, as a send does, and has it
  // back, with why, if the fork doesn't go.
  const [forking, setForking] = useState(false);
  const fork = (prompt: string) => {
    if (forking) return;
    const key = s.key;
    const typed = text;
    // It goes back where it was typed: the session's draft or its side conversation's.
    const from = draft;
    setForking(true);
    setNote(null);
    setText('');
    forkSession(key, { background: true, prompt })
      .then(
        (started) => void land(key, { text: started?.message ?? 'Forked in the background.', error: false }),
        (e: unknown) => {
          const why = errorText(e);
          writeDraft(from, [typed, readDraft(from)].filter((t) => t.trim()).join('\n\n'));
          if (!land(key, { text: why, error: true })) onFailed?.(why);
        },
      )
      .finally(() => setForking(false));
  };

  // `/side`, and the question after it. The box clears at once and the question shows above it
  // as soon as its copy is started; one that can't be asked is back in the box, which stays the
  // side conversation's, with why. With no question it shows the side conversations kept from
  // before.
  const [asking, setAsking] = useState(false);
  const askAside = (question: string) => {
    if (asking) return;
    const key = s.key;
    setNote(null);
    setText('');
    // Out in a window of its own, that comes forward to show it, leaving the keyboard here. It's
    // up already: only a window gone meanwhile fails, and the next `side-windows` puts it back
    // above this box.
    if (popped) void sideWindow(key, false).catch(() => {});
    if (!question) {
      showSides(key, { kind: 'list' });
      return;
    }
    // The open one shows, whatever was being read, and until the engine says its copy started
    // the box is still the side conversation's, in every window.
    startingSide(key);
    setAsking(true);
    // The question shows as the engine tells of it (side.ts), as its answer does: what this
    // resolves with can be behind what's been heard since.
    sideAsk(key, question)
      .catch((e: unknown) => {
        const why = errorText(e);
        const side = sideDraft(key);
        writeDraft(side, [question, readDraft(side)].filter((t) => t.trim()).join('\n\n'));
        notAsked(key);
        if (!land(key, { text: why, error: true })) onFailed?.(why);
      })
      .finally(() => setAsking(false));
  };

  /** Runs what's in the box when it's one of Codealong's commands, else sends it: to the session,
   *  or to its side conversation while that shows. */
  const submit = (): boolean => {
    const command = slashCommand(text, s);
    if (!command && aside) {
      if (!text.trim()) return false;
      askAside(text.trim());
      return true;
    }
    if (!command) return send();
    if (command.command.name === 'fork') fork(command.rest);
    if (command.command.name === 'side') askAside(command.rest);
    return true;
  };

  // A pasted screenshot or file goes in at the cursor, on a line of its own; the agent gets its
  // path (send.rs expands it).
  const paste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = filesToAttach(e);
    if (files.length === 0) return;
    e.preventDefault();
    const at = e.currentTarget.selectionStart;
    keepFiles(files)
      .then((kept) => {
        const r = insertAttachments(latest.current, at, kept);
        setText(r.text);
        window.requestAnimationFrame(() => {
          box.current?.focus();
          box.current?.setSelectionRange(r.cursor, r.cursor);
        });
      })
      .catch((err: unknown) => setNote({ text: errorText(err), error: true }));
  };

  const empty = !text.trim();
  // A command goes even while the session waits on you: it doesn't reach the session.
  const command = slashCommand(text, s);
  const commands = slashMatches(text, s);
  // While a name is being typed at the caret, what it could be: Codealong's own commands (at
  // the start of the box, where they run), then the session's skills, less any Codealong's
  // commands take the name of. Escape puts the list away until the name changes. Not in a side
  // conversation's box, which takes what's typed as words.
  const taken = new Set(COMMANDS.flatMap((c) => [c.name, ...(c.aliases ?? [])]));
  const options: SlashOption[] =
    !mention || aside
      ? []
      : [...(mention.start === text.length - text.trimStart().length ? commands : []).map((c) => ({ name: c.name, hint: c.args, about: c.about, own: true })), ...matching(skills.filter((k) => !taken.has(k.name)), mention.name).map((k) => ({ name: k.name, hint: k.hint, about: k.description }))];
  const [closedFor, setClosedFor] = useState<string | null>(null);
  const [picked, setPicked] = useState(0);
  const listing = options.length > 0 && closedFor !== mention?.name;
  const at = Math.min(picked, options.length - 1);
  /** Finishes the name at the caret as the option's, with the cursor after it. */
  const complete = (o: SlashOption) => {
    if (!mention) return;
    const next = completed(text, mention, o);
    setText(next.text);
    setCaret(next.caret);
    setPicked(0);
    window.requestAnimationFrame(() => {
      box.current?.focus();
      box.current?.setSelectionRange(next.caret, next.caret);
      // Focusing puts the old selection back for a moment, which the box reports.
      setCaret(next.caret);
    });
  };
  // A skill the box names whole, with what may follow it, for the line under the box.
  const skill = !command && !listing ? skillNamed(text, skills) : undefined;
  // Replies suggested for it, above the box, while they're the session's: not while the box is a
  // side conversation's, a message is on its way, or the slash list has the room.
  const suggested = s.suggestions && s.suggestions.replies.length > 0 && !aside && !going && !listing && answered !== s.suggestions.at ? s.suggestions : null;
  // While it waits on a call that can't be answered here, a reply can only go in the box.
  const suggestionBlocked = waiting && !onDenySaying ? 'It waits on you for something that can’t be answered from here, so this can go once it doesn’t' : undefined;
  const pickSuggestion = (reply: string, how: PickHow) => {
    if (how === 'edit') {
      const now = latest.current.trim() ? `${latest.current.trimEnd()}\n\n${reply}` : reply;
      setText(now);
      window.requestAnimationFrame(() => {
        box.current?.focus();
        box.current?.setSelectionRange(now.length, now.length);
      });
      return;
    }
    const next = how === 'next' && !!onNext;
    // Sent and on to the next as ⌘⏎ does: a deny goes on once it's in.
    if (send(false, next, reply) && next && !waiting) onNext?.('sent.');
  };
  const dismissSuggested = () => {
    const at = s.suggestions?.at;
    if (at === undefined) return;
    setAnswered(at);
    box.current?.focus();
    dismissSuggestions(s.key).catch((e: unknown) => {
      setAnswered(null);
      setNote({ text: errorText(e), error: true });
    });
  };
  /** Gives the keyboard to the reply nearest the box, from ↑ in the empty box. */
  const intoSuggestions = (): boolean => {
    const replies = suggestionList.current?.querySelectorAll<HTMLButtonElement>('.ca-suggest-reply');
    const last = replies?.[replies.length - 1];
    last?.focus();
    return !!last;
  };
  // With nothing in the box, the button stops a turn that can be stopped; with something, it
  // sends, the turn taking the message in as it works. Not while the box is the side
  // conversation's: what's done there leaves the session's turn alone.
  const stopButton = canStop && empty && !aside;
  // Into the turn it's taking at once, where it can be reached that way.
  const canSendNow = !!s.steerable && !waiting && !aside;
  const said = note?.error
    ? note.text
    : going
      ? 'Sending…'
      : forking
        ? 'Forking…'
        : switching
        ? 'Switching its model…'
        : asking
          ? 'Asking a copy of it…'
          : listing
          ? '↑ ↓ choose · ⇥ finishes the name · esc puts the list away'
          : commands.length > 0
          ? commands.map((c) => `/${c.name} ${c.args}: ${c.about}`).join(' · ')
          : skill
          ? `/${skill.name}${skill.hint ? ` ${skill.hint}` : ''}: ${skill.description}`
          : (note?.text ?? (stopping ? 'Stopping…' : onlyAside ? SESSION_SIDES.open : aside ? `To the side conversation, not ${s.account}. Close it or pop it out above to message ${s.account}.` : hint(waiting, !!onNext, !!onAllow, canSendNow, !!onDenySaying, !empty, !!suggested && !text)));
  return (
    <div className={cx('ca-composer', aside && 'is-aside')}>
      {listing && <SlashList options={options} active={at} onPick={complete} onHover={setPicked} />}
      {suggested && (
        <SuggestedReplies
          ref={suggestionList}
          suggestions={suggested}
          onPick={pickSuggestion}
          onDismiss={dismissSuggested}
          blocked={suggestionBlocked}
          denies={waiting && !!onDenySaying}
          next={!!onNext}
          onLeave={() => box.current?.focus()}
        />
      )}
      <div
        className="ca-composer-field"
        onMouseDown={(e) => {
          // A press anywhere in the box but its buttons and its menus puts the cursor in it.
          if ((e.target as Element).closest('button, textarea, .ca-picker')) return;
          e.preventDefault();
          box.current?.focus();
        }}
      >
        {attached.length > 0 && <AttachmentStrip items={attached} onRemove={(a) => setText(withoutAttachment(latest.current, a.path))} />}
        <BoxText
          ref={box}
          value={text}
          lines={LINES}
          hint={aside ? 'Ask on the side' : waiting && onDenySaying ? `Or deny it, and tell ${s.account} what to do instead` : replyPlaceholder(s, terminal, way, resumeWhere) + (canStop ? ' · esc stops its turn' : '')}
          onChange={(e) => {
            setText(e.target.value);
            setCaret(e.target.selectionStart);
            setPicked(0);
            if (note && !note.error) setNote(null);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onKeyDown={(e) => {
            // The list's keys come first while it's up: the arrows choose, ⇥ finishes the name,
            // and so does ⏎ unless the name is already whole, which sends it as ⏎ does. Escape
            // puts the list away, and stops nothing.
            if (listing && !e.nativeEvent.isComposing && !e.metaKey && !e.ctrlKey && !e.altKey) {
              const whole = options[at].name === mention?.name;
              if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                e.preventDefault();
                setPicked((at + (e.key === 'ArrowDown' ? 1 : options.length - 1)) % options.length);
                return;
              }
              if ((e.key === 'Tab' && !e.shiftKey) || (e.key === 'Enter' && !e.shiftKey && !whole)) {
                e.preventDefault();
                complete(options[at]);
                return;
              }
              if (e.key === 'Escape') {
                e.preventDefault();
                e.stopPropagation();
                setClosedFor(mention?.name ?? null);
                return;
              }
            }
            // ↑ in the empty box goes to the replies suggested above it, and nowhere else: the
            // window's own keys would read it as the list's once the box no longer has the keyboard.
            if (e.key === 'ArrowUp' && !text && suggested && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.nativeEvent.isComposing && intoSuggestions()) {
              e.preventDefault();
              e.stopPropagation();
              return;
            }
            // Escape stops a running turn; once it's stopping, Escape does what it does in the
            // window (closes the peek, gives Sessions' list its keys back).
            if (e.key === 'Escape' && canStop && !aside && !stopping && !e.nativeEvent.isComposing) {
              e.preventDefault();
              stop();
              return;
            }
            // ⌘⌫ with nothing written denies the call it waits on; with words, it deletes them as
            // it does in any box.
            if (e.key === 'Backspace' && e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey && onDeny && !aside && !text.trim()) {
              e.preventDefault();
              onDeny();
              return;
            }
            if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return;
            // To the side conversation, whatever the keys: none of the session's own (send now,
            // send and go on to the next, allow) apply to it.
            if (command || aside) {
              e.preventDefault();
              submit();
              return;
            }
            // ⌃⏎ sends it now; the engine sends it as usual where it can't, or the turn has ended.
            if (e.ctrlKey && !e.metaKey) {
              e.preventDefault();
              send(true);
              return;
            }
            // While it waits on a call answered here, ⌘⏎ answers it and goes on: what you wrote
            // denies it and is sent instead, as with ⏎, and an empty box allows it.
            if (e.metaKey && waiting && onDenySaying && text.trim()) {
              e.preventDefault();
              send(false, true);
              return;
            }
            if (e.metaKey && onAllow && !text.trim()) {
              e.preventDefault();
              onAllow();
              return;
            }
            // While it waits on you ⏎ denies it with what you wrote, where it can be answered here
            // (and sends nothing elsewhere), and ⌘⏎ goes on to Allow (the waiting window's).
            if (waiting && e.metaKey) return;
            e.preventDefault();
            if (send() && e.metaKey) onNext?.('sent.');
          }}
          onPaste={paste}
          aria-label={aside ? `Question on the side about ${s.title}` : `Message to ${s.title}`}
        />
        <div className="ca-composer-buttons">
          {resuming && account && !aside && resuming.places.length > 1 && <ResumePlace picks={resumePicks} chosen={resuming} onPicks={setResumePicks} onDone={() => box.current?.focus()} />}
          {resuming && account && !aside && resuming.takesAModel && <ResumeModel account={account} picks={resumePicks} chosen={resuming} onPicks={setResumePicks} onDone={() => box.current?.focus()} />}
          {saysModel && !aside && <SessionModel session={s} account={account} switches={switches} busy={switching || s.status === 'working' || !!s.stalledSince || !!waiting} onSwitch={switchModel} onDone={() => box.current?.focus()} />}
          {canSendNow && !empty && !command && (
            <button type="button" className="ca-composer-send is-now" aria-label="Send now" title="Send now (⌃⏎), into the turn it’s taking" onClick={() => send(true)}>
              <Icon name="bolt" size={14} stroke={2} />
            </button>
          )}
          {stopButton ? (
            <button type="button" className="ca-composer-send is-stop" aria-label={stopping ? 'Stopping' : 'Stop'} title="Stop its turn (esc)" disabled={stopping} onClick={stop}>
              <Icon name="player-stop" size={12} stroke={2} />
            </button>
          ) : (
            <button
              type="button"
              className="ca-composer-send"
              aria-label={command ? `Run /${command.command.name}` : aside ? 'Ask on the side' : 'Send'}
              title={command ? `Run /${command.command.name} (⏎)` : aside ? 'Ask the side conversation (⏎)' : waiting ? (onDenySaying ? 'Deny it, and send this instead (⏎)' : 'Send this once it isn’t waiting on you') : 'Send (⏎)'}
              disabled={empty || (waiting && !command && !aside && !onDenySaying) || ((!!command || aside) && (forking || asking))}
              onClick={submit}
            >
              <Icon name={command?.command.icon ?? (aside ? 'help' : 'arrow-up')} size={16} stroke={2} />
            </button>
          )}
        </div>
      </div>
      {said && (
        <span className={cx('ca-composer-note', note?.error && 'is-error')} role="status">
          {said}
        </span>
      )}
    </div>
  );
}

/** What the box has to say when nothing's happened: only the keys that aren't a chat box's usual
 *  ones. ⏎ sends and ⇧⏎ starts a new line, as everywhere, and the placeholder says Escape stops a
 *  turn while it can. */
function hint(waiting: boolean | undefined, next: boolean, allow: boolean, sendNow = false, denySaying = false, written = false, suggested = false): string {
  const keys = hintKeys(waiting, next, allow, sendNow, denySaying, written);
  return suggested ? [keys, '↑ suggested replies'].filter(Boolean).join(' · ') : keys;
}

function hintKeys(waiting: boolean | undefined, next: boolean, allow: boolean, sendNow: boolean, denySaying: boolean, written: boolean): string {
  if (waiting && denySaying && written) return `⏎ denies it and sends this instead${next ? ' · ⌘⏎ + next' : ''}`;
  if (waiting && denySaying) return allow ? `⌘⏎ allow${next ? ' + next' : ''} · ⏎ denies it and sends what you write instead` : '⏎ denies it and sends what you write instead';
  if (waiting) return allow ? `⌘⏎ allow${next ? ' + next' : ''} · send this once it isn’t waiting on you` : 'Send this once it isn’t waiting on you';
  return [next && '⌘⏎ send + next', sendNow && '⌃⏎ send now, into the turn it’s taking'].filter(Boolean).join(' · ');
}

interface ApprovalProps {
  session: Session;
  terminal: string;
  onOpen: () => void;
  /** After an answer went through, with what happened. */
  onAnswered?: (said: string) => void;
  /** ⌘⏎ allows and ⌘⌫ denies from anywhere in the window, where the keyboard is the way in
   *  (the waiting window, the notch). */
  keys?: boolean;
  /** Hands the box the call's keys (null while there's nothing to answer here): ⌘⏎ allows,
   *  then goes on to the next with `onNext` where there is one, and ⌘⌫ denies. */
  onKeys?: (keys: CallKeys | null) => void;
  onNext?: (done: string) => void;
}

/** What the box's keys do to the call a session waits on. */
interface CallKeys {
  /** None for a call that can't be allowed just this once (`Pending.noOnce`). */
  allow?: () => void;
  deny: () => void;
  /** Denies it, telling it `words` instead, then with `next` goes on as an allow does: why that
   *  didn't go, or null once it did. */
  denySaying: (words: string, next: boolean) => Promise<string | null>;
}

/** Whether a key press is someone editing words, which ⌘⌫ deletes there. */
function editingText(target: EventTarget | null): boolean {
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return target.value.trim() !== '';
  return target instanceof HTMLElement && target.isContentEditable;
}

/** A session waiting on you: what it wants to do, with Allow and Deny when it can be answered here. */
function Approval({ session: s, terminal, onOpen, onAnswered, keys, onKeys, onNext }: ApprovalProps) {
  const [pending, setPending] = useState<Pending | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  useEffect(() => {
    let live = true;
    sessionPending(s.key)
      .then((p) => live && setPending(p))
      .catch(() => live && setPending(null));
    return () => {
      live = false;
    };
  }, [s.key, s.statusSince]);

  /** Answers the call; resolves with why that didn't go, or null once it did. A deny with
   *  `message` tells it what to do instead. */
  const answer = (a: 'allow' | 'deny' | 'always', then?: (said: string) => void, answers?: Answers, message?: string): Promise<string | null> => {
    if (!pending || busy) return Promise.resolve(busy ? 'It’s still answering the last one.' : 'It isn’t waiting on anything Codealong can answer.');
    setBusy(true);
    setNote(null);
    return sessionAnswer(s.key, pending.id, a, answers, message)
      .then((said) => {
        setNote({ text: said, error: false });
        setPending(null);
        onAnswered?.(said);
        then?.(said);
        return null;
      })
      .catch((e: unknown) => {
        const why = errorText(e);
        setNote({ text: why, error: true });
        return why;
      })
      .finally(() => setBusy(false));
  };

  const asks = !!pending?.questions?.length;
  // Questions take picks, so ⌘⏎ doesn't allow them from the box.
  const answerable = !!pending && !pending.cannot && !asks;
  // Nor does it allow a call its agent lets through only from now on.
  const allowable = answerable && !pending.noOnce;
  const allowRef = useRef(answer);
  allowRef.current = answer;
  // Where it goes on to next is read once the answer is in, as it is then.
  const nextRef = useRef(onNext);
  nextRef.current = onNext;
  /** After an answer ⌘⏎ gave: on to the next that waits on you, where there's one to go on to. */
  const andNext = (said: string) => nextRef.current?.(said);
  useEffect(() => {
    if (!keys || !answerable) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !e.metaKey || e.altKey || e.ctrlKey || e.shiftKey) return;
      // With the keyboard anywhere in the window, as from the box: ⌘⏎ allows and goes on, and
      // ⌘⌫ denies, unless it's deleting words you wrote.
      if (e.key === 'Enter') {
        e.preventDefault();
        if (allowable) void allowRef.current('allow', andNext);
      } else if (e.key === 'Backspace' && !editingText(e.target)) {
        e.preventDefault();
        void allowRef.current('deny');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [keys, answerable, allowable]);
  useEffect(() => {
    if (!onKeys) return;
    onKeys(answerable ? { allow: allowable ? () => void allowRef.current('allow', andNext) : undefined, deny: () => void allowRef.current('deny'), denySaying: (words, next) => allowRef.current('deny', next ? andNext : undefined, undefined, words) } : null);
    return () => onKeys(null);
  }, [onKeys, answerable, allowable]);

  const where = sessionPlace(s, terminal);
  // Questions that can only be answered where it runs still show here, to read before you go.
  if (pending?.cannot && pending.questions?.length) {
    return (
      <div className="ca-composer is-approval is-questions is-elsewhere">
        <div className="ca-approval-head">
          <Icon name="help" size={14} />
          <span>Asks you</span>
        </div>
        <QuestionList questions={pending.questions} />
        <div className="ca-composer-foot">
          <span className="ca-composer-note" title={pending.cannot}>
            {note?.text ?? `Answer ${pending.questions.length > 1 ? 'them' : 'it'} in ${where}.`}
          </span>
          <Button variant="primary" size="sm" icon="arrow-up-right" onClick={onOpen}>
            Open in {where}
          </Button>
        </div>
      </div>
    );
  }
  // A held message that can only be answered where it runs still shows here, to read first.
  if (pending?.cannot && pending.message) {
    return (
      <div className="ca-composer is-approval is-elsewhere">
        <div className="ca-approval-head">
          <Icon name="message-circle" size={14} />
          <span>Holds a message for you</span>
        </div>
        <div className="ca-approval-label">
          <Rich text={pending.label} now={0} />
        </div>
        {pending.input && <Markdown text={pending.input} className="ca-approval-plan" />}
        <div className="ca-composer-foot">
          <span className="ca-composer-note" title={pending.cannot}>
            {note?.text ?? `${pending.cannot} Answer it in ${where}.`}
          </span>
          <Button variant="primary" size="sm" icon="arrow-up-right" onClick={onOpen}>
            Open in {where}
          </Button>
        </div>
      </div>
    );
  }
  if (!pending || pending.cannot) {
    const why = s.stalledSince ? `It looks stuck. Open it in ${where} to see what it’s waiting for.` : `It’s waiting on you. Answer it in ${where}.`;
    return (
      <div className="ca-composer is-waiting">
        <span>
          {note?.text ?? why}
          {pending?.cannot ? ` ${pending.cannot}` : ''}
        </span>
        <Button size="sm" icon="arrow-up-right" onClick={onOpen}>
          Open
        </Button>
      </div>
    );
  }
  if (pending.questions?.length) {
    // What it wrote with them, when its record doesn't have that yet: the conversation above
    // stops short of it, and where it runs already shows it.
    const unwritten = pending.unwritten ? `What ${s.account} wrote with this question shows in the conversation once it’s answered${s.hosted ? '' : `; ${where} has it now`}.` : null;
    return (
      <Questions
        questions={pending.questions}
        unwritten={unwritten}
        where={s.hosted ? null : where}
        onOpen={onOpen}
        keys={keys}
        busy={busy}
        note={note}
        onAnswer={(answers, on) => answer('allow', on ? andNext : undefined, answers)}
        onDecline={() => answer('deny')}
      />
    );
  }
  // A plan or a held message reads as prose, and its answers say what they do to it.
  const plan = !!pending.plan;
  const message = !!pending.message;
  const says = message
    ? 'Deliver gives it to the session as that session’s message. Deny drops it, and its sender is told it was declined.'
    : plan
    ? 'Allow approves the plan. Deny keeps it planning, or write below what to change.'
    : pending.noOnce
      ? `Its agent ${pending.always ? 'allows calls like this only from now on, not just this once' : 'offers no way to allow this'}. Deny tells it no, and it carries on.`
      : s.hosted
        ? 'Allows this call once. Deny tells it no, and it carries on.'
        : 'Allows this call once. Deny stops the turn, or write below what to do instead.';
  return (
    <div className="ca-composer is-approval">
      <div className="ca-approval-head">
        <Icon name={message ? 'message-circle' : 'alert'} size={14} />
        <span>{message ? 'Holds a message for you' : plan ? 'Wants you to approve its plan' : 'Wants to run'}</span>
        {!s.hosted && (
          <button type="button" className="ca-linkbtn" onClick={onOpen}>
            Open in {where}
          </button>
        )}
      </div>
      {!plan && (
        <div className="ca-approval-label">
          <Rich text={pending.label} now={0} />
        </div>
      )}
      {pending.input && pending.input !== pending.label && (plan || message ? <Markdown text={pending.input} className="ca-approval-plan" /> : <CodeBlock text={pending.input} className="ca-approval-input" />)}
      <div className="ca-composer-foot">
        <span className={cx('ca-composer-note', note?.error && 'is-error')}>{busy ? 'Answering…' : (note?.text ?? says)}</span>
        <Button size="sm" kbd={['⌘', '⌫']} disabled={busy} onClick={() => answer('deny')}>
          Deny
        </Button>
        {pending.always && (
          <Button size="sm" disabled={busy} title={pending.alwaysRule ?? 'Allows this call, and calls like it from now on.'} onClick={() => answer('always')}>
            Always
          </Button>
        )}
        {!pending.noOnce && (
          <Button variant="primary" size="sm" icon="check" kbd={['⌘', '⏎']} disabled={busy} onClick={() => answer('allow')}>
            {message ? 'Deliver' : 'Allow'}
          </Button>
        )}
      </div>
    </div>
  );
}

interface QuestionsProps {
  questions: Question[];
  /** That what it wrote with them isn't in the conversation yet, and where it is, in a sentence. */
  unwritten: string | null;
  /** Where the session runs, for its Open link; none for one Codealong runs. */
  where: string | null;
  onOpen: () => void;
  /** ⌘⏎ answers, where the keyboard is the way in (the waiting window). */
  keys?: boolean;
  busy: boolean;
  note: { text: string; error: boolean } | null;
  /** Your picks; `on` when ⌘⏎ gave them, which goes on to the next that waits on you as it
   *  does from the box. The button answers and stays. */
  onAnswer: (answers: Answers, on: boolean) => void;
  onDecline: () => void;
}

/** Questions a session asks you (Claude Code's AskUserQuestion): pick a choice, several where a
 *  question takes more, or type your own answer; your picks go back as its answer. Several show
 *  one at a time, as Claude Code's own prompt shows them, with a strip of their headers to go
 *  between them: all at once, a few long ones ran off the bottom of the panel. */
function Questions({ questions, unwritten, where, onOpen, keys, busy, note, onAnswer, onDecline }: QuestionsProps) {
  const [picks, setPicks] = useState<Record<string, string[]>>({});
  const [own, setOwn] = useState<Record<string, string>>({});
  // The question showing, for the questions it was picked among: another set starts at the first.
  const asked = questions.map((q) => q.question).join('\n');
  const [step, setStep] = useState({ asked, at: 0 });
  const at = step.asked === asked ? Math.min(step.at, questions.length - 1) : 0;
  const go = (i: number) => setStep({ asked, at: i });
  const answers: Answers = {};
  for (const q of questions) {
    const words = (own[q.question] ?? '').trim();
    const chosen = picks[q.question] ?? [];
    if (q.multiSelect) {
      const all = words ? [...chosen, words] : chosen;
      if (all.length) answers[q.question] = all;
    } else if (words || chosen.length) {
      answers[q.question] = words || chosen[0];
    }
  }
  const ready = Object.keys(answers).length > 0;
  const answered = questions.map((q) => q.question in answers);
  // Where Next goes, while another question has no answer; with none, the button answers.
  const next = nextQuestion(answered, at);
  // The answer box to take the keyboard once Enter there went on to the next question.
  const ownRef = useRef<HTMLInputElement>(null);
  const typing = useRef(false);
  useLayoutEffect(() => {
    if (!typing.current) return;
    typing.current = false;
    ownRef.current?.focus();
  }, [at]);
  const pick = (q: Question, label: string) => {
    setPicks((all) => {
      const chosen = all[q.question] ?? [];
      const next = q.multiSelect ? (chosen.includes(label) ? chosen.filter((l) => l !== label) : [...chosen, label]) : [label];
      return { ...all, [q.question]: next };
    });
    // One answer to a question that takes one: a choice clears what was typed, and is all the
    // question takes, so the next one without an answer shows.
    if (!q.multiSelect) {
      setOwn((all) => ({ ...all, [q.question]: '' }));
      if (next !== null) go(next);
    }
  };
  const type = (q: Question, text: string) => {
    setOwn((all) => ({ ...all, [q.question]: text }));
    if (!q.multiSelect && text.trim()) setPicks((all) => ({ ...all, [q.question]: [] }));
  };
  const answerRef = useRef((_on: boolean) => {});
  answerRef.current = (on) => {
    // ⌘⏎ does what the button does: on to the next question while one has no answer.
    if (next !== null) go(next);
    else if (ready && !busy) onAnswer(answers, on);
  };
  useEffect(() => {
    if (!keys) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !e.metaKey || e.altKey || e.ctrlKey || e.shiftKey || e.key !== 'Enter') return;
      // Not from words being written elsewhere (the message box below): that's a message, and
      // answering would send whatever was left half typed here instead. An answer box of its own
      // takes ⌘⏎ before it gets here.
      if (editingText(e.target)) return;
      e.preventDefault();
      answerRef.current(true);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [keys]);
  const q = questions[at];
  return (
    <div className="ca-composer is-approval is-questions">
      <div className="ca-approval-head">
        <Icon name="help" size={14} />
        <span>Asks you</span>
        {where && (
          <button type="button" className="ca-linkbtn" onClick={onOpen}>
            Open in {where}
          </button>
        )}
      </div>
      {unwritten && <p className="ca-approval-unwritten">{unwritten}</p>}
      {questions.length > 1 && (
        <Seg
          className="ca-question-steps"
          label="Questions"
          options={questions.map((q, i) => ({ id: i, label: q.header || `Question ${i + 1}`, title: q.question, icon: answered[i] ? 'check' : undefined }))}
          value={at}
          onPick={go}
        />
      )}
      <div key={q.question} className="ca-question">
        <div className="ca-approval-label">
          {/* With several, the strip above names each by its header. */}
          {q.header && questions.length === 1 && <Chip className="ca-question-header">{q.header}</Chip>}
          <Rich text={q.question} now={0} />
        </div>
        <div className="ca-question-choices" role={q.multiSelect ? 'group' : 'radiogroup'} aria-label={q.question}>
          {q.options.map((o) => {
            const on = (picks[q.question] ?? []).includes(o.label);
            return (
              <Button key={o.label} size="sm" variant={on ? 'primary' : 'secondary'} role={q.multiSelect ? 'checkbox' : 'radio'} aria-checked={on} title={o.description} disabled={busy} onClick={() => pick(q, o.label)}>
                {o.label}
              </Button>
            );
          })}
        </div>
        <input
          ref={ownRef}
          className="ca-question-own"
          type="text"
          placeholder={q.options.length ? 'Or type your own answer' : 'Type your answer'}
          value={own[q.question] ?? ''}
          disabled={busy}
          onChange={(e) => type(q, e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== 'Enter' || e.altKey || e.ctrlKey || e.shiftKey || e.nativeEvent.isComposing) return;
            if (e.metaKey) {
              // ⌘⏎ from an answer you typed, in every window: on to the next question, or, with
              // none left, it answers and goes on.
              e.preventDefault();
              answerRef.current(true);
            } else if (next !== null) {
              // Enter goes on to the next question and its box; it never answers, so a half-typed
              // answer isn't sent.
              e.preventDefault();
              typing.current = true;
              go(next);
            }
          }}
        />
      </div>
      <div className="ca-composer-foot">
        <span className={cx('ca-composer-note', note?.error && 'is-error')}>{busy ? 'Answering…' : (note?.text ?? (q.multiSelect ? 'Pick one or more. Decline stops the turn.' : 'Pick one, or type your own. Decline stops the turn.'))}</span>
        <Button size="sm" disabled={busy} onClick={onDecline}>
          Decline
        </Button>
        {next !== null ? (
          <Button variant="primary" size="sm" kbd={keys ? ['⌘', '⏎'] : undefined} disabled={busy} onClick={() => go(next)}>
            Next
          </Button>
        ) : (
          <Button variant="primary" size="sm" icon="check" kbd={keys ? ['⌘', '⏎'] : undefined} disabled={busy || !ready} onClick={() => answerRef.current(false)}>
            Answer
          </Button>
        )}
      </div>
    </div>
  );
}

export interface ReplyProps {
  session: Session;
  terminal: string;
  /** Opens the session where it lives, to answer what can't be answered here. */
  onOpen: () => void;
  autoFocus?: boolean;
  keys?: boolean;
  onSending?: () => void;
  onSent?: (sent: Sent) => void;
  onFailed?: (why: string) => void;
  onAnswered?: (said: string) => void;
  /** Going on to the next session that waits on you: ⌘⏎ in the box sends, or allows the call this
   *  one waits on, then goes on, saying what was done. */
  onNext?: (done: string) => void;
}

/** A session's side conversation in a window of its own (kept.rs `pop_side`, Peek.tsx): the
 *  conversation, filling the window, and the box that asks it, which is the side conversation's
 *  whatever shows. */
export function SideReply({ session, terminal, inWindow }: { session: Session; terminal: string; inWindow: SideWindowCan }) {
  return (
    <>
      <SideStrip key={`side:${session.key}`} sideKey={session.key} words={SESSION_SIDES} inWindow={inWindow} />
      <Composer key={`box:${session.key}`} session={session} terminal={terminal} autoFocus onlyAside />
    </>
  );
}

/** The bottom of a session's panel: what it waits on you for, if anything, its side conversation
 *  while it has one, and the box that sends it a message. The box is the same one throughout, so
 *  it keeps what's in it as the session starts and stops waiting. */
export function Reply({ session, terminal, onOpen, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onNext }: ReplyProps) {
  const waiting = !takesReply(session);
  // What the box's ⌘⏎ and ⌘⌫ do, once there's a call to answer here.
  const [call, setCall] = useState<CallKeys | null>(null);
  const takeKeys = useCallback((keys: CallKeys | null) => setCall(keys), []);
  const callKeys = waiting ? call : null;
  return (
    <>
      {waiting && <Approval key={`wait:${session.key}`} session={session} terminal={terminal} onOpen={onOpen} onAnswered={onAnswered} keys={keys} onKeys={takeKeys} onNext={onNext} />}
      <SideStrip key={`side:${session.key}`} sideKey={session.key} words={SESSION_SIDES} />
      {/* A session Codealong runs takes a message while it waits, after its current turn. */}
      <Composer key={`box:${session.key}`} session={session} terminal={terminal} autoFocus={autoFocus} waiting={waiting && !session.hosted} onSending={onSending} onSent={onSent} onFailed={onFailed} onNext={onNext} onAllow={callKeys?.allow} onDeny={callKeys?.deny} onDenySaying={callKeys?.denySaying} />
    </>
  );
}
