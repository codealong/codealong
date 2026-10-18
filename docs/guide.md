# The Codealong guide

Everything Codealong does, in detail: every view, window, menu and key, and what Codealong does
with each harness. New to Codealong? Start with the [README](../README.md), the short tour: what
it is, how to install it and how to get going. The settings file is annotated in the README's
[Settings](../README.md#settings) section, and building from source and the code map are in
[development.md](development.md).

## Contents

- [The main window](#the-main-window)
- [Finding and starting work](#finding-and-starting-work)
- [Answering agents from anywhere](#answering-agents-from-anywhere)
- [Opening things where they live](#opening-things-where-they-live)
- [What Codealong works out](#what-codealong-works-out)
- [Tidying up](#tidying-up)
- [Your other devices](#your-other-devices)
- [For agents](#for-agents)
- [Files Codealong keeps](#files-codealong-keeps)
- [What's next](#whats-next)

## The main window

**Six views** of the current project, each in a window of its own if you like (⌘⇧N). The project
switcher (⌘P) shows each project's key: ⌥⌘1 to ⌥⌘9 switch projects, ⌥⌘0 shows them all. Add
project… at its foot opens Settings › Projects with a new project's form, and Edit projects… the
list. ⇧⌘S, ⇧⌘W,
⇧⌘P, ⇧⌘T, ⇧⌘K and ⇧⌘B go straight to Sessions, Worktrees, PRs, Todo, Kanban and Canvas (B for
board: ⇧⌘C is often a clipboard manager's), from a text field too, and a view in the sidebar shows its keys while
you point at it (the folded rail says them); ⌃Tab and ⌃⇧Tab go through
the views (`v` too, outside Sessions), ⌥↑ ⌥↓ move along a list as the arrows do, and ⌃⌘↑ ⌃⌘↓ reorder
what's pinned. ⌘F finds where you are, as VS Code's does: from a session's panel (the keyboard in
it, or a click in it last) the words in its conversation, from a PR's panel its files, and from
anywhere else, the list say, the list's find, which narrows the view's list (*Find in the list*).
⌥⌘F opens the list's find from anywhere, as in Mail and Notes, and ⌘K, or the box in the middle of
the title bar, opens the quick panel, to find a session, a todo or a PR anywhere or to start one.
The two icons at the title bar's right end are New session (+, ⌘N) and Improve Codealong (the
bulb). ⌃- goes back to the session you were looking at before and ⌃⇧- forward
again, as in
VS Code, from any view or text field; the two arrows beside the sidebar's toggle do the same and say
where they go. ⌃` opens a new terminal in the folder of what's shown, as VS Code opens its terminal,
from any view or text field and in every window that shows a session: the session's own folder, else
the selected worktree's, or the worktree of the todo or PR selected. A session counts once its
conversation has shown for a second, so the ones passed over on the way down a list aren't stops,
and each window keeps its own history across launches. A session's or worktree's actions have
letters (`p` pin, `a` archive, `r` rename, `e` mark seen) and chords that work in every view (⌃⌘P,
⌃⌘A, ⌃⌘R, ⌘E, and three with no letter of their own: ⌃⌘W closes a session's terminal, ⇧⌘E marks it
done, and ⌃⌘1 to ⌃⌘4 snooze it for one of the Snooze menu's times); the right-click menu shows them.
A click on a row's chips selects the row, as a click anywhere on it does, and a double-click does
the chip's own thing (the session where it runs, its todo, its app) in place of opening the row; in
a panel, a click does. A PR's chip is the same wherever it is, a row, a panel or a card: a click
shows the PR in Codealong, and a double-click opens it on GitHub; a PR Codealong doesn't list opens
on GitHub on a click too. In Sessions, Worktrees and Todo, the PR's panel shows in place of the one
beside the list, the chip's row selected, with a link back to the row's own panel over it, so the
list stays as it was; from PRs, the canvas (its card, when the canvas has one) and the floating
windows, it's the PR's row in PRs. A click waits a quarter of a second for a second one, so a
double-click doesn't show the PR first. After the number, a mark says where its review stands:
approved, changes asked for, your review asked for (amber), or Codealong's agent still reviewing
it; a chip that says its state in words (a session's header) marks only the last two.
A right-click (or ⌃-click) opens a row's menu and leaves the selection where it was; the row is
ringed while its menu is up. A right-click on the header of a
panel (a session's, a worktree's, a PR's, a todo's, or the rows picked together) is its row's menu,
a todo's title too unless you're typing in it. In every list ⌘-click picks a row beside the selected
one or leaves it out, ⇧-click (or ⇧↑ ⇧↓ from the list) picks a run and ⌘A picks every row: the panel
then lists them with what's commonly done to all of them at once, a right-click on one of them is
the menu for all of them, the keys act on all of them, and Esc goes back to one. Chips are one shape
and size in every list, and none is cut off: on a row short of room a long name shortens inside its
chip, and the last chips drop out whole.

### Sessions

Every session, under headings as the other views have them. Pinned ones come first in your order
(drag or ⌃⌘↑ ⌃⌘↓). Drag any session up among them to pin it there,
and a pinned one down to the rest to unpin it. The bar over the list has two groups of icon buttons,
each named in its tooltip. Sort by orders the rest: Status puts Needs you, Your turn and Working
first, as the floating bar orders them, then Snoozed, the soonest back first, then the rest under
the day they were last active (Today, Yesterday, Last 7 days, Older), the one you last sent a prompt
first. Started puts the newest session first, under the day it started, and each row's age then
says when it started. A row leads with the session's title, then what it's doing or said last; one
that needs you has a label beside its title for what it waits on (Asks you, Needs approval,
Message, Stuck, Error). Its second line says where and why: its worktree, its todo and Jev's read,
then its account, with the worktree's PR and diff at the right. Rows, after Sort by, puts each
session on one line instead (its title, then its status, worktree and PR chips at the line's end);
there its status says its account when you point at it. A snoozed session's row says how long until it's
back, and a quiet one's says Quiet. Here letters alone do nothing, so a word typed on the list by
mistake doesn't pin, archive or rename: the keys are the chords, and they work from the panel's box
too. Sessions are pinned here, with ⌃⌘P or the right-click menu, and their pins show only here.

#### Archive, close and mark as done

Archive a session with ⌃⌘A (`a` elsewhere) or its right-click menu; Codex threads are archived in
Codex itself. A message you send an archived session, or a resume, takes it out of the archive as
it goes (a Codex thread out of Codex's too, since Codex resumes no archived thread); an agent's
message through the `codealong` command leaves it there. Close terminal, in the right-click menu of a session running in a Ghostty, iTerm2 or
Terminal tab, closes that tab whatever the session is doing, and End background job stops a Claude
Code background job; either way its conversation is kept and resumes. Mark as done (⇧⌘E, or Done on
the strip) is for a session you're finished with: it closes the session's terminal or ends its
background job, as Close does, and marks it seen and archives it, so it leaves every list but
Archived. It closes nothing that would take more than the session with it: a terminal that shows
other sessions too stays open, and a session in the Claude or Codex app is only archived, since the
app has no way for another program to close it. A background job that holds the worktree Claude
Code made for it is stopped, and the worktree is removed after it, as Clean up worktree… does (see
[Clean up worktrees](#clean-up-worktrees)), unless something in it would be lost: then it stays, and
the toast says why ("1 commit on no remote"). A session still working, or one that left a command
running in the background, isn't ended by surprise: Mark as done says so, and pressed again within
15 seconds ends that too. A session on another Mac is done there, when that Mac lets its linked
devices act on its sessions (remote control); otherwise it's only marked seen and archived, and the
toast says why its terminal is still open. Here the toast says what was done, and its Undo takes it
out of the archive; the windows that float (the bar, the notch, the waiting window) say only what
didn't go as asked, such as a terminal left open, since the session leaving their list says the
rest.

#### Picking several

⌘-click (or ⇧-click, ⇧↑ ⇧↓ from the list, ⌘A for all) picks several sessions: the panel lists them
with Seen, Done, Snooze, Close and Archive for all of them at once, each saying how many it takes
where that's fewer, a right-click on one of them is the menu for all (Pin, Quiet, Stop turn and Copy
session IDs too), and the same chords act on all of them; Esc goes back to one. Archive and Mark as
done have one Undo for the lot. With a search typed, Sessions' Archived fold lists the archived
sessions it finds, so its count says how many match.

#### Emblems

With Jev on for emblems (Settings › Jev, `[jev] emblems`), each session gets a small icon beside its
title, so the sessions beside each other read apart at a glance and you can remember one as "the
teal bug". Jev picks the icon for what the session works on, from its title and first prompt (the
latest, for Codex and OpenCode, which keep only that), once: it stays for the session's life,
however its title changes. The icon sits on a rounded square shaded in two colors that go
together, as if lit from one of eight places around its edge: a main color over most of it, and an
accent where the light falls. There are thirteen colors, evenly round the color wheel: the label
colors and rose, magenta, violet, sky, mint and lime between them. A pair is any two of them at
most four steps apart, close ones included (blue and sky, teal and lime, pink and indigo…), never
two that clash. Codealong gives each session its pair and its light when it first sees it, so that the
sessions of a project differ as much as they can: a main color the fewest of its recent sessions
have, then a pair, then a light none of the sessions in that color has. When you give a session a
color with Claude Code's `/color`, that's its accent, with a main color that goes with it. The
colors have no amber, since amber only means something needs you, so `/color`'s yellow shows as
lime and its orange as red. The emblem shows wherever the session's title does: its rows, its header,
the floating bar's and the notch's pills, cards on the canvas and the desktop, and the list of what
waits on you. A session gets its emblem once it has a prompt, and only while it's recent: one quiet
for two days or archived keeps the one it had. Emblems are kept in `emblems.json`.

#### Rename

Rename a session with ⌃⌘R (`r` elsewhere) or its right-click menu, here or wherever a session has
one (the floating bar's pills and cards on the desktop change the name in place, and the quick
panel's rows in the waiting window). The name goes where its harness keeps names, so the harness
shows it too: a Claude Code session's transcript, which `/resume` lists it by and a running session
takes up, in its terminal's title too, the next time it reads its transcript's end; a Codex thread,
which the Codex app shows the next time it reads its threads (not an archived one); an OpenCode
session, through its service, so OpenCode must be running. Antigravity's can't be renamed here.

#### Stop a turn

Stop turn, in the right-click menu of a session whose turn is running (here, on the floating bar and
in the quick panel), ends that turn and leaves the session waiting for you, as its own Stop or
Escape would: Escape pressed in its Ghostty, iTerm2 or tmux tab for Claude Code and the Codex CLI
(through its account's background service for a Claude Code background job), the Codex app's own
Stop for its threads, and OpenCode's service for OpenCode. Claude app sessions, Terminal tabs and
Antigravity can't be stopped from Codealong.

#### Seen and unread

Mark a finished turn seen, which makes it idle and takes it off the floating bar, with ⌘E (`e`
elsewhere), the check at the top of its panel or its right-click menu. Only that marks it: a Claude
Code turn you haven't seen stays Your turn for three days after its terminal closes or the Mac
restarts, and a Codex thread's until you see it. Mark as unread (⌃⌘U, `u` elsewhere, or its
right-click menu, for several picked sessions too, and from a linked phone) makes a turn you've seen
Your turn again, until you see it or send it a prompt.

#### Fork

Fork in terminal, in a Claude Code, Codex or OpenCode session's right-click menu, opens a copy of
its conversation in a new terminal while it carries on (OpenCode's copy is made by its service, so
it needs OpenCode running). `/fork` in the box at the bottom of its panel forks it without a window
coming forward, and you stay on the session: a Claude Code copy is a background job, an OpenCode
copy runs in its service, and a Codex copy runs In Codealong (over its ACP adapter). Whatever
follows `/fork` is the copy's first prompt; with nothing, the copy waits for one.

#### Move to another account

Move to, in a Claude Code session's right-click menu, moves it to another of your Claude Code
accounts (one that's signed in): the conversation goes on in a new terminal under that account (a
background job's as a job there), and the session is closed and archived; its panel and the copy's
say where it went. The copy is a session of its own, with a new ID, and takes the session's place on
its todos, among the pinned sessions, on its card on the desktop and in its kept window. Moved from
its panel in Sessions, the panel stays on it while it moves and then shows the copy, selected in the
list, rather than going on to the next that waits on you; a PR opened from its chip meanwhile stays,
its way back going to the copy. A session moving, or moved, offers no second move in the window it
was moved from, so a double-click doesn't make two copies. Claude Code
writes nothing of the copy until you send it something, so until then it shows the conversation it
was moved with; its terminal closed before then, it stays on the board, stopped, and opening it or
sending it a message makes it again from that conversation. A move is noted before the copy starts,
and a terminal that doesn't open leaves the session where it was. A turn it's taking is stopped
first, so the copy goes on from where it was interrupted; a command its last turn left running in
the background stays behind. A message sent to the old session afterwards, from a window or another
agent's `codealong session send`, is turned away with the copy named, and `codealong session show`
says where it moved. The first reply there reads the whole conversation again, since the old
account's prompt cache doesn't carry over.

#### Usage limits

A session whose turn its account's usage limit stopped says so (Limit, with which limit and when it
lifts, from what Claude Code or Antigravity wrote). A Claude Code session's panel offers to move it
to the account with room: one of its harness's other accounts, chosen as Auto chooses, that isn't
near a limit by its meters and whose own sessions weren't refused since they last got through (a
spend limit shows on no meter).
Moved, it's told to carry on where it stopped. `[limits] when_hit = "move"` (Settings › Automation)
moves it there by itself, as soon as it stops; a session moved there that stops again is offered a
move rather than moved back.

A session still stopped at its limit, and still open where it stopped, is told to carry on a few
minutes after the limit lifts, and its Limit says so. Claude Code may carry on by itself first (its
own Continue automatically at usage limit, 30 to 90 seconds after); then Codealong leaves it be. A
session stopped again straight after it was told a few times running is left for you, with a
notice. `[limits] continue_when_lifted = false` (Settings › Automation) turns that off.

#### Network errors

A Claude Code session, or an Antigravity one in its command line, whose turn ended because it
couldn't reach its API says so (Network error): with the lid closed the Mac wakes for a few seconds
every minute or two without DNS, and Claude Code's retries run out there; Antigravity gives up at
once on a connection the network dropped while it waited on a permission prompt or a command.
Codealong tells it to carry on once the API can be reached again, once for each time, and leaves it
for you (with a notice) when it loses the connection again straight after a few times running;
`[sessions] continue_after_network_error = false` (Settings › Automation) turns that off. Any other
error that ends an Antigravity turn shows as one (Error), in its own words, until you've seen it.

#### Cut off

A Claude Code or Codex session whose turn stopped partway because what ran it ended (its terminal
closed or quit, Codealong or the Mac restarted, it was killed) says so (Cut off), and waits on you until you've
seen it; a session that had finished its turn when that happened lost nothing, and is just stopped.
Its panel has Carry on, which resumes it where its account's sessions last went on from here, else
in a terminal, and tells it to carry on, checking first what its last step did, since a command it
ran may have been stopped partway. Resume in, in its menu, says Carry on in and does the same
anywhere it can go on; sending it something resumes it with what you send. A Claude Code session is
read so in a terminal or a background job (its transcript ends partway through a turn, with no
process running it); a Codex thread when its turn is open and no Codex process has it loaded; and
a session Codealong runs itself whose turn was under way when Codealong quit or its agent's process
ended, which its agent's own record ends as stopped (Codealong notes each such turn while it runs,
in hosted.json).

A session Codealong runs itself is its agent's, and its agent is a process of Codealong's, so
Codealong quitting (an update, or a new build relaunched) ends the turns it was taking. Once
Codealong is back it tells each such session to carry on, at once and without waiting on you: the
message picks it up again in a new process of its agent, says it was cut off, and that a call the
cut ended may read as one you rejected, which you didn't. A session that last did something more
than an hour before is left for you, since Codealong was gone a while. Each cut is told once; a
session cut off again straight after a few times running (its agent dies as it's picked up) is
left for you, with a notice. An automatic review is carried on by its round instead (*Automatic
reviews*).
`codealong session wait` on such a session (a Codex helper, say) waits through the restart and
the carrying on, rather than coming back with the cut. `[sessions] continue_after_restart = false`
(Settings › Automation) turns that off.

#### Move to another worktree

Move to worktree, in the same menu (in the main window, the quick panel and the waiting window),
moves a Claude Code session that runs in a terminal or in the background, or isn't running, to
another worktree on this Mac (listed by project) or to a chat of its own. It's closed where it runs
and goes on in the new folder as itself, with the same ID, so it keeps its todos, its pin and its
card: a terminal session in a new terminal there, a background job as a job there. It's told where
it is now, and coming from a chat it can still read the chat's folder. Claude Code copies a
background job that has been one before into a new job rather than move it, and the copy takes its
place as a move to another account's does.

#### Side conversations

`/side <question>` (or `/btw`) asks something about a Claude Code or Codex session on the side: a
throwaway copy of its conversation answers above the box, and the session carries on and never hears
of it. The copy reads files and runs commands to answer, and is told to change nothing unless your
question asks for that change in so many words, as Codex's own `/side` is; then it can do what the
session itself can without asking (a Claude Code session that skips permission prompts, a Codex
thread's sandbox), and what would have to ask you is refused. The harness keeps nothing of the copy:
no transcript, no session on the board. While it shows, the box under it asks the side conversation,
not the session (its placeholder says Ask on the side): type a follow-up and press ⏎. Close gives
the box back to the session at once. A copy that's still answering when you close it answers what it
was asked before it ends, so closing one to talk to the session loses no answer: it stays listed
under the session as answering, and a click on it shows it to read as the answer comes (Delete there
stops it). A new question after a close starts a new copy, which knows none of the closed one. A copy
left idle for 15 minutes ends by itself, and a closed one that nothing comes from for 15 minutes is
stopped. Codealong keeps what was asked and answered, with what the copy did to answer
(`sides.json`, the latest 300): `/side` alone, or Earlier beside an open one, lists a session's
side conversations from before, and each opens to read, or to delete; while they show, a question
in the box starts a new one. A
session's header names each under its title, with the sessions that came of it (*Where a session
came from*), and a click on one shows it above the box. In Sessions, the one answering is listed
under its session, set in as a helper at work is, and a click on its row selects the session with
the side conversation above its box. The session's row counts them on a chip of their own (a
question mark and how many, with the working glyph while one answers); resting the pointer on it
brings up a card with each, where it stands since when (*Answering for 1m*, *Answered 2h ago*),
and a click on one there shows it the same way. A double-click on the chip, or the card's button,
lists them all under the session, answered ones too, until a row at the end of what's listed,
*Hide 2 answered side conversations*, or another double-click puts them away. What can be done with one is a row of
icons at the strip's top right (Earlier, Delete, Pop out, Close), each named on hover. Pop out moves
it into a window of its own beside the one it was in, which floats as a session kept on the desktop
does: the conversation fills it, its box asks only the side conversation, and the session's boxes go
back to the session, so you can talk to both at once (`/side` in the session's box still asks the
side conversation, and brings its window forward). Its top line drags it and its edges size it; Put
back closes the window and shows the conversation above the box again, and its × or ⌘W close it,
as Close does, and the window. It comes back at launch where it was left, empty until you ask it something. Other
`/…` commands go to the session as typed.

#### The session panel

The selected session's panel keeps its top to three lines so the conversation has the room: the
title with the ways to open it, mark it seen, expand the panel, its details (ⓘ: account, ID, folder,
times) and its menu (⋯, New session here among the rest); where it stands, its worktree and branch
(its account is under the reply box, below, or ends this line where there's no box: a session on a
device that can't be reached from here); and the PRs it worked on (as the PRs view links them, else
its worktree's), each a chip that shows the PR's panel in place of the session's on a click (in
the floating windows, in PRs), opens it on GitHub on a double-click and shows the PR's card on
hover, with the todo it's on and +N for the rest. Beside its PRs is the latest page on the web it
made: an artifact Claude Code published, made from a type or opened, its subagents' included, as a
chip with its title that opens it in the browser on a click, what it last did with it and when on
hover. When it made more, "+N more" beside the chip (naming them on hover) lists the rest under the
links, the latest first, each with what was last done with it and how long ago, and "fewer" puts
them away, as the sessions that came of it go under its title. An artifact published again is one
page, as it last stood. Below is its whole conversation, read from the session's own record
(Claude Code's transcript, Codex's rollout, OpenCode's database): your prompts, the agent's replies
as Markdown, and every tool call, which opens to its input and output; runs of calls fold into one
line. Who said what and when heads a run of messages, not each one: your prompt, then the agent's
reply, whose later messages between its calls go on under that heading; it heads one again after a
note in the conversation or a pause of ten minutes (the agent woken by a background job's report).
Another session's message (an agent's, sent with `codealong session send`) shows whole, under who
it's from and set apart by a rule at its left, in any harness: one Claude Code took on its
cross-session socket, and one typed into a session of another harness, without the line saying
whose it is. A file or an image sent with it shows as in a prompt of yours, a chip or the image in
place. A long one shows its first ten lines, fading out, until Show all (Show less cuts it again),
and shows whole while ⌘F finds something in it. A device whose build is from before gets the note
it showed, the message's first line under who it's from; so does the phone, as a note. An artifact Claude Code published shows under its call as its title, a link that opens it,
with what the call did (Published · version 3), and stays out of a fold; web links in a tool's
output open too. Files Claude Code sent you (its SendUserFile tool, which the Claude app and your
phone show as cards) are listed under their call, each by its name and folder, with what the agent
said of them, and the call stays out of a fold. Click one to open it in the app that opens its
kind, as a double-click in the Finder would; an app, a script or a program is shown in the Finder
instead, so a click never runs anything. A file sent from a scratch folder is often cleared by the
time you look, and says so where you clicked. A session on another Mac has its files there, so
they're listed but don't open. A prompt shows the images you sent with it in place, whether you sent them from
Codealong, pasted them into the session's terminal or attached them in the Claude or Codex app;
click one to open it. It opens at the newest and loads older pages as you scroll up, and follows the
session as it goes. A link to a GitHub PR, in a reply or a prompt, reads `name#123` with the PR's
state as its icon; hovering it shows a card with where the PR stands (review, checks, branches,
size, and whose reviews it still waits on, you first), the worktrees that hold it and the sessions
that worked on it, each a click from its panel, and its right-click menu is the PR's. A PR the
board doesn't list (an older one, another repo's) is asked of GitHub when its card first shows.
What Codex's app draws in a reply is drawn here too, near enough. A writing block, a finished piece
of writing such as an email, a message to paste elsewhere or a document, is a card named for its
kind, with Copy, which copies the writing without the lines around it. An email shows its recipients
and subject over it, and when Codex offers versions (Friendly, Direct) you pick one at the top. A
code comment is a card with the file and lines it's about. A file Codex cites shows its name and
folder, and opens on a click as a file sent you does. A follow-up it offers is a button that puts its prompt in the session's box, after what's
there, to send or change.
A card the pointer brings up (a PR's, a session's helpers) goes away on a key, which still does
what it does, but not on ⌘, ⇧, ⌥ or ⌃ alone, nor on ⌘C, which copies what you selected on it.

#### Replies as they're written

A session's record has a reply only once the agent has written all of it, so the panel would show
it only then. A session Codealong runs shows its reply as it's written, under its account and
"writing", until its record has it. With Settings › Agents › Show Claude Code replies as they're
written (`[sessions] live_replies`), so does a Claude Code session in any terminal or background
job: the Codealong plugin gets a `MessageDisplay` hook that hands Codealong each paragraph as the
terminal shows it. It also shows what the agent wrote with a question that waits on you, which
Claude Code often saves only once the question is answered; without the setting, the question says
its terminal has that text. The cost is in the terminal: with any such hook, Claude Code shows a
reply a paragraph at a time rather than a line at a time, so a paragraph's start shows there a
moment later, though the reply ends at the same time. It applies to sessions started after it's
turned on, since Claude Code reads hooks as a session starts, and Codealong updates the plugin in
every Claude Code account that has its copy when you change it.

#### While it works

While a session's agent works, the end of its conversation says so, wherever the conversation
shows: the Codealong mark in gray, its color gliding slowly round it, then a word for what the agent's up to (Tinkering…, Untangling…,
Sharpening pencils…) and how long it has worked on what you last asked. The word is for fun, not news; a new one
comes every 30 seconds, each session with its words in an order of its own, the same in every
window, and a turn past a quarter of an hour gets one that says so now and then (Still at it…).
A session gone silent mid-turn has no line: its header says how long it's been silent. One whose
turn is over but that waits on a command it left running in the background (a CI watch, say)
says so in the line's place, with the clock the list draws for it: Waiting on and the command,
all of it on hover.

Now and then, in the main window, a tip on using Codealong sits quietly on the line's right
(*Tips*), beside the word, or under it on the right when the panel is too narrow for both: only
once a turn has gone on 20 seconds, and no more than one every 20 minutes, wherever the last
showed. A turn keeps the tip it got until it ends, when you look at another session and come back,
in another window, and while other turns get theirs.

#### Moving through a conversation

⌘⇧↑ and ⌘⇧↓ bring your previous or next prompt to the top, from the box too (⌘↑ and ⌘↓ as well while
the cursor isn't in a text box), reading further back as they go; the prompt behind what you're
reading stays pinned at the top once it has scrolled off, two lines of it and more on hover, even
when a long turn put it pages back (they're read then, and kept for scrolling up), and a click
brings it up.

#### Find

⌘F in its panel finds words in it, wherever the panel shows: a bar over the conversation, in place
of the prompt pinned there, marks every match and counts them across the whole conversation (the
pages further back are read for it), starting from the one nearest where you're reading; ⏎ goes to
the match before, on into earlier pages, ⇧⏎ to the one after (⌘G and ⇧⌘G from anywhere in the
window), a run of tool calls opens to show a call it finds, and Escape closes the bar and gives the
keyboard back. It finds what shows: prompts, replies and the calls' lines, and a call's input and
output once you've opened it. A session that a search found by what was said in it, in the list's
find or the quick panel, opens with the bar already on the words it found (the words as you
typed them when the line it was found by has them together, else the longest of them that line
has), at the nearest match, the keyboard staying where it was.

#### Copy and Run

A right-click on a message has Copy message, which copies it as it was written, Markdown and all,
and Copy for what you'd selected in it; in the peek, the notch, the waiting window and a kept
session, the session's own menu follows. Code blocks have Copy on hover, and a shell block in a reply (`bash`, `sh`, `zsh`…) or a command the
agent ran has Run beside it: that runs the commands without their comments in a new tab of your
terminal, where the session works or where the command ran, as a command in your terminal does
(*Commands in your terminal*): in your login shell, the tab closing by itself a while after they end
unless you press Return there for a shell. A block with no language counts as shell
when each of its lines starts with a command on your PATH (`git`, `npm`, `cd`…), so output, a path or
a config file keeps Copy alone. A block led by `!` is one an agent writes for you to run with what
it prints coming back to it (a sign-in, a command that needs you): Run runs it as `!` in the
session's box does (*Replying*), in a tab of yours with the session told what it printed. A command
you ran with `!` in Claude Code's own terminal shows as yours with its output, and Run runs it
again the same way.

#### Replying

A message that starts with `!` is a command rather than words, for any harness: ⏎ runs it in a new
tab of your terminal, in the folder the session works in, as you (*Commands in your terminal*), and
once it ends the session gets a message from you with the command, how it ended and the last of
what it printed, so its agent reads it as Claude Code's own `!` has it read what a command printed.
What the box says (*What the box says*) says so as you type it, and its button shows a terminal. It runs whatever the
session is doing, as it doesn't reach the session until it ends; a session on another device takes
it as a message, as before. A message that opens with an image isn't a command, its `!` being the
image's.

The box at the bottom sends it a message where it runs, as the waiting window does; paste a
screenshot or a file into it and the agent gets its path, and what you'd written to a session and
not sent stays with that session until you send it, in every window (the panel, the floating bar's
peek, the waiting window) and across restarts. A session whose last reply asks you something (Jev's
read) takes your answer there like any message. A Claude Code turn stopped before any reply takes
its prompt back, so the agent never sees it: the panel shows that prompt faint, marked "Taken back",
and puts it back in the box, as Claude Code does in its own. The next message Codealong types into
that terminal clears Claude Code's copy first (not in Terminal, which can't press a single key).
A message you send shows faint at the end of the conversation, Sending… and then Sent, until the
session's record has it. One its harness holds until the turn that's running ends says Queued until
this turn ends for as long as it waits: Antigravity's command line queues what's typed while it works
(its `Queued Messages` setting, Queue by default), and its record has the message only once the turn
is over. The Codex app's queue (when Codex is set to queue follow-ups) shows whole at the end of the
thread, after the turn that's running, whoever queued it: from Codealong, from the Codex app's own
box, or from another device, and still there after Codealong restarts. One the session shows no sign of having taken, once it's been quiet a while, says Not in the
session yet, with Open to check. One that may have gone in without Codealong being able to tell (its
terminal didn't confirm the typing, say) counts as sent rather than going back in the box, where it
would invite sending it twice: the box says where to look, and the session's record says whether it
went in.

At the left of the box's foot, under where you write, are the mode a running session is in, the
session's account (its harness on hover) and a small pie for its context window; its model and the
send button are at the right. The mode
is how much it does without asking you, in its harness's own words, said lowercase, with what
that means on hover (and in the header's Details): Claude Code's Manual, Plan, Accept edits, Auto,
Don't Ask or Bypass (a terminal session's from its transcript, where a switch shows
from its next prompt; the Claude app's from the app's record); Codex's Read Only, Ask for approval,
Approve for me (its reviewer agent answers for you) or Full Access, and Plan, from its latest
turn; OpenCode's agent (Build, Plan or one of yours); Cursor's Ask or Plan, and in its other modes
its auto-run setting (Run Everything, Auto-review, Allowlist, Ask Every Time), the chat's own or
the app's; Antigravity's tool permission from its latest turn (Request review, Strict, Auto, Proceed
in sandbox, Always proceed, which `--dangerously-skip-permissions` also gives). Antigravity's
accept-edits and plan modes aren't written where Codealong reads, so they don't show. Codealong only
says the mode of a session that runs elsewhere: it's changed there. A session Codealong runs itself
starts in the mode its account's settings give new sessions (Claude Code's `permissions.defaultMode`
in the account's `settings.json`: without one it's Manual, even where Claude Code's terminal starts
in Auto), and its mode is a menu of the modes its agent offers (Claude Code's Manual, Accept edits,
Plan, Auto and Bypass); ⇧⇥ in its box steps to the next, as in Claude Code's terminal. Either
switches that session alone, mid-turn too and while it waits on you: what it asks from then on goes
by the new mode, and what it asks now still waits for your answer. The pie says how full the context window is,
as its latest call left
it, counted as its harness counts it for its own meter: Claude Code's input,
cache reads and cache writes against the model's window (a million tokens for the 1M models),
Codex's tokens against the window its rollout names, OpenCode's last message. It turns amber past
80% and red past 95%, when the agent is about to compact or run out of room; where the window
isn't known it shows the tokens. Hovering it shows the tokens of the window, the model, and what the
session has cost so far at API prices (see *What sessions used and cost*). Cursor and Antigravity
don't say, so their sessions have none.

#### Approvals and questions

When a session waits on you to allow a tool call, the call shows above the box with Allow (⌘⏎) and
Deny (⌘⌫, with nothing written in the box, and the session stays shown so you can say what to do
instead): a session Codealong runs is answered through its agent, and calls it asks about together
(Claude Code reading several files at once) show one after another; OpenCode's are answered through its service, Claude Code's through the Codealong plugin's
`PermissionRequest` hook when the account has the plugin (wherever the session runs: any terminal,
the Claude app), with Always too where Claude Code offers to allow calls like it from now on (its
button says what that adds), and otherwise by a keypress in its Ghostty, iTerm2 or tmux tab, only
while its own record says the prompt is up for that call. An Antigravity command line conversation
in a terminal is answered the same way, by a keypress there while its database says the call
waits: Allow is its prompt's first choice (run the command, read the file) and Deny cancels the
prompt, which ends its turn; its other choices (always, for the conversation or in its settings)
are left to its terminal, since they're numbered differently from one prompt to another.
Questions a Claude Code session asks you
(its `AskUserQuestion`) show the same way through the plugin, one at a time under a strip of their
headers, each with its choices listed as Claude Code lists them, what each says about itself under it,
and a box for your own answer, and Answer sends your picks. In a card too short for all of it (the
peek under a tall header, say), the conversation gives up its room first, then the question and its
choices scroll together while your own answer's box and the buttons stay in view. Claude Code's own prompt stays up all the while, and
whichever answer comes first counts. An Antigravity command line conversation's questions (its
`ask_question`) show the same way while it runs in a Ghostty, iTerm2 or tmux tab: Answer presses
the keys there that give your picks, typing your own words into its Write-in box, and Decline
skips them all and stops its turn. A question with more than eight choices is answered in its
terminal. What you'd begun answering in its terminal is cleared first, except on the last
question, where a choice you ticked there and pick here too ends up unticked; the conversation
shows the answers it took. Another session's message that a Claude Code session holds for
you to allow (it skips permission prompts and the sender didn't say it does too) shows the same way,
all of it when Codealong sent it, with Deliver and Deny, pressed in its Ghostty, iTerm2 or tmux tab
or typed into its background job; with several held, it opens where it runs, which shows them one at
a time. Others open where they run to be answered. The box stays meanwhile, keeping what you'd
written. Where the call can be answered here, sending what you wrote (⏎) denies it and tells the
agent that instead, as Claude Code's own "No, and tell Claude what to do differently" does, and the
panel shows your words as a message of yours; nothing is typed at a prompt that's up. Questions are
declined the same way, so you can write back in your own words rather than pick an answer: the
agent's turn goes on with what you wrote (an answer of your own to one question still goes in that
question's box, and Answer sends it). Elsewhere Send
works again once the session isn't waiting (a session run In Codealong takes it at once, after its
current turn). An answer that lets the session go on (Allow, Always, Deliver, your picks, or a deny
with words) shows it working in every window as you give it, before its agent has said it went on;
one that doesn't go brings the call back, saying why. After a plain Deny the session shows as its
agent has it once that's read: Claude Code ends the turn there, others carry on.

#### What the box says

What the box has to say goes in the box itself, on the line at its foot, between who the message
goes to and the buttons: how a message went (Sending…, then what Codealong did with it, or why it
didn't go, in red), what what you've typed will do (a `!` command, a `/` command or skill, the side
conversation), and, where the window is worked from the keyboard (Sessions, the notch, the waiting
window), the keys that aren't a chat box's usual ones (⌘⏎ send + next, ⌃⏎ send now, ↑ for suggested
replies, what ⏎ and ⌘⏎ do while it waits on you). It stays on that one line: what doesn't fit is cut
short with an ellipsis, and resting the pointer on it shows all of it over the box. On the canvas,
in the floating bar's peek and in a session kept on the desktop the keys aren't said, as the
buttons' tooltips have them.

#### Stop and send now

The box's send button sits inside it, at its corner, and its placeholder says how a message gets
there: typed into its Ghostty, iTerm2, Terminal or tmux tab, pasted into its background job, as a
message from Codealong, or through its app. While a turn runs that Codealong can stop, Escape in the
box stops it, as it does in Claude Code and Codex (⌘. anywhere in the window too), and so does the
button while the box is empty; it says Stopping until the turn has ended. While a turn runs, ⏎ sends
as the session's own box would (Claude Code takes it in once its current step ends, the Codex app
into the turn, or queued to start once it ends when Codex is set to queue follow-ups, and OpenCode
after the turn), and Send now (the bolt beside Send, or ⌃⏎ as in Claude Code) sends it
into the turn at once where the session can take it so: Claude Code's send now in its terminal or
background job, which moves a running command to the background so the agent reads it straight away,
and Codex's and OpenCode's steer.

#### Working through what waits on you

To work through what waits on you, ⌘⏎ in the box sends and goes on to the next session listed that
waits on you (Needs you, then Your turn, each the longest wait first, so one that comes back after
you answered it waits behind the rest), or, while the session waits on a call that can be allowed
here, allows it and goes on; ⏎ sends and stays. ⌘⏎ on an answer you typed to a session's questions
answers and goes on too. ⌘E (or the check at the top of the panel) marks a finished turn seen and
goes on, and the keys stay where they were, so ⌘E works down the list or from box to box; from a
session that doesn't wait on you (one that runs, say) it goes on as ⌘] does. Snoozing, closing or
archiving a session that waits on you goes on to the next as well, by its key, the strip's button or
its menu. ⌘] or ⌥⇧↓ skips to the next, ⌘[ or ⌥⇧↑ goes back to the one before, and a skipped one
comes round again after the rest. A message that doesn't go after ⌘⏎ went on is back in its
session's box, and that session comes round again: the strip over the one you went on to says why,
rather than a toast over the box you're typing in. The window's toasts sit at the foot of the list,
clear of the panel. ⇧⌘F (or the button beside the check) expands the panel to the
whole window, the list and sidebar out of the way while you work through them, and stays so until
you press it again; Escape in the box then leaves the keys with the window.

#### The strip

A strip at the top of the panel says how the last one went, at its left, on one line beside its
buttons, cut short where they leave it too little room, with all of it under the pointer (what the
box has to say is in the box: *What the box says*). It has what's commonly done to a session as
buttons that say their keys: Seen (⌘E), Done (⇧⌘E), Snooze (⌃⌘1 to ⌃⌘4, one for each of its menu's times: 15
minutes, 1 hour, 3 hours, the next morning), Close (⌃⌘W, which closes its terminal, or ends a
background job where the session is one; the conversation stays, to go on with later), Archive (⌃⌘A)
and Next (⌘]). One that doesn't apply to the session shown is dimmed in its place. Clicking a
session puts the cursor in its box; ⌥↑ ⌥↓ from there move along with the cursor in the next one's
box, the arrows on the list leave the keys with the list, and Escape in the box gives them back.

### Worktrees

Each worktree once, pinned ones first in your order, then the rest by status: Needs you, Working,
Your turn, In review, Idle, Merged. A row reads as where, then who. On its left, shaded as one
block, the worktree and what belongs to it: its name, then Free when it's a reusable worktree new
work can start in, and its app (a globe, green while it runs); its repo and branch, then its PR; and
beside them where it stands against the default branch: lines added and removed (a dot for
uncommitted files) over commits ahead and behind, dropped on a narrow board. On its right, its
agents, a line each, every line a session: each one that needs you, is working or just finished, or
with none of those, the one that last worked there (a worktree no agent has worked in says so). You
select, open or right-click a line as you would a row, and its todo, Jev's read and its account are
at its end. The worktree's panel lists the rest of its sessions and todos. An agent is listed under
the worktree it's working in, where most of its latest commands ran and edits landed, even if it
started somewhere else; resuming it still happens where it started. Sessions outside your projects
are listed under Elsewhere, in the repo or folder each ran in, and show in All projects only while
they need you. Chats you start without a folder in the Claude app, or outside a project in the Codex
app, are one worktree for each app, Claude app chats and Codex chats, shown like your projects'
worktrees. Needs you is a queue: the worktree and the agent that have waited longest come first, so
newer ones can't keep an old one down (worktrees that need you only through snoozed agents come
after). Needs you, Working and Your turn are about agents: a worktree whose PR has failing checks or
changes requested stays in In review, and the sidebar's PR needs work lists it. Within the other
statuses, the worktree and the agent you last sent a prompt come first, so rows hold still while
agents work. Stuck agents (busy but silent for 30 minutes) show in Needs you with the command
they're stuck on. The sidebar's statuses narrow each worktree to the agents they're about. Pin a
worktree with `p`, the right-click menu of the worktree or one of its agents, or by dragging its row
up to the top of the board, among the pinned ones; reorder pins by dragging or with ⌃⌘↑ ⌃⌘↓, and
drag one down to the bays to unpin it. A pinned worktree stays whatever the age filter says. Beside
its line diff, a row shows how far the worktree has moved from the default branch on the remote
(`origin/main`), whatever its branch tracks: `↑3` for commits it has that the remote doesn't, `↓42`
for commits the remote has that it doesn't. Canvas cards show the same. Worktrees are picked whole,
from any part of their row: picked together they take Mark as seen for their sessions (`e`), Pin
(`p`), Reuse for new work, Clean up worktrees… and copies of their paths and branch names. A
worktree's right-click menu has Clean up worktree… beside Reuse for new work, dimmed with why when
it can't go ([Clean up worktrees](#clean-up-worktrees)).

### PRs

Every PR in the project's repos, with the worktree and sessions that worked on each: the ones that
made, edited or commented on it, or pushed to it (from their own records: Claude Code's
`gitOperation` results and its `pr-link` lines, Codex's commands and attached PRs), the ones on its
branch while it was open, any in the repo whose records show they pushed to its branch from a
branch of their own (`git push origin mine:theirs`), and any linked by hand (`codealong pr
link|unlink`). One of them is the PR's primary session, listed first and tagged Primary on its card
and in its panel, whose tooltip says why it's the one:

- the one picked, under Primary session in the PR's menu or with `codealong pr nudge --session`;
- else the one that pushed to the PR's branch most while it was open (each session's count is in
  `codealong pr show`). The one that opened the PR (the first whose records mark it) counts as
  having pushed once at least, since its push may have left no record (a quiet `git push -q`, a
  harness that records none), and wins a tie: a session that pushes a fix or two to another's PR
  doesn't take it over, but one that goes on with the work does;
- else the one that started first.

Pushes are read from Claude Code's records (`gitOperation.push`), which name the branch but not
the repo, so only sessions in the PR's repo count.

Until the project's PRs have come from GitHub (each repo's first read, and the search for the PRs
that wait on your review), a view with none of them yet says it's reading them, with a tip, rather
than that there are none. When GitHub can't be asked (`gh` missing or signed out, a rate limit), it
says why instead.

#### Nudging a PR's primary session

A PR's primary session can be nudged with what happens on it. It's off for a PR until turned on
for it, with the switch on the PR's card (hover its chip in a session's header, or a link to it in
a conversation), Nudge its session on updates in its menu, or by an agent: `codealong pr nudge
<pr>` makes the agent's own session the one told, unless another running session is the PR's
primary (`--take` takes it over, `--off` stops it). Settings › Automation › Nudge the session that
opened a pull request turns it on for every open PR of yours (`[pull_requests]
nudge_by_default`), and a PR's own switch wins over that either way.

The primary session, while it runs, is then sent a message, as one of yours, when someone else
reviews the PR, asks for changes or comments (with what they wrote, read from GitHub as the PR
changes), when it's approved, merged or closed, when its checks fail, and when its branch comes to
conflict with its base (someone merged into the base what it conflicts with), with where it
stands (ready to merge, its checks are still running), so it takes it from there. A conflict is
told once, and again only if it comes back after it was resolved; one resolved before the
message could go isn't told. The message never asks
it to merge, an approval included: it says so, and the agent merges only if you asked it to. The
PR's other sessions (helpers, reviewers, ones that came to its branch) aren't told, and what you
did to it yourself, which is what its agent did through `gh`, isn't told back. Turned on for a PR
that's approved already, it's told that at once (not when the setting turns it on for every PR);
what happened while Codealong wasn't running isn't told. An agent's review that Codealong posted
from someone's account (see *Automatic reviews*) never passes for their word, since it comes as a
message of yours: a comment Codealong posted is told as their agent's, naming it; a review of theirs
with their agent's review in it (which may hold words of theirs too) is told as having it, naming
each agent, and its line comments as maybe the agent's. Codealong knows one by its footer and unseen
mark at the start of their lines, which nothing in a review hides; a footer quoted within a line
leaves someone's words their own, and one pasted whole at the start of its lines is taken for an
agent's, the safer mistake.

Settings › Automation › What a nudge tells it about (`[pull_requests] nudge_about`) narrows that,
for every PR with its nudge on: `"everything"` (the default) is all of the above; `"feedback"` is
only what asks for work, someone else's comments and reviews that ask for changes or say
something, so the session can work their feedback in; `"feedback-and-checks"` adds failing checks
and conflicts.
With either, an approval (its own words with it), a dismissed review, a merge and a close aren't
told, nor anything once the PR has ended, and the PR's card says so under its switch.

A primary session on this Mac that has stopped (its terminal closed, or Claude Code's background
service ended it after an hour idle) is resumed out of sight to be told, as a session is resumed
for another session's message: under its harness's background service, else in Codealong. It
starts with a line saying it had stopped and Codealong resumed it to tell it this. A Codex app
thread, which the app takes a message for while it isn't running, is sent it there. Only news
that asks for work goes to one that isn't running: someone else's comments and reviews that say
something, changes asked for, failing checks and a conflict, never an approval, a merge or a close,
whatever the nudge tells of while it runs. It isn't resumed while another of the PR's sessions runs
on this Mac, or a session runs in a worktree on the PR's branch on another of your devices that's
connected, unless that one is a helper or a review, since the work may have gone on there and two
agents on one branch get in each other's way; then nobody is told until it runs. Nor is one you archived or marked done, or one on an account you turned off, and a
Claude app session, which goes on only in its app, isn't told until it runs. Resumed as a copy
(Claude Code copies a session a background job had into a new job), the copy becomes the PR's
primary session. The PR's card says which of these it is under its switch. Settings › Automation ›
Resume a stopped session for its pull request's news (`[sessions] resume_for_pr_news`, on unless
turned off) turns it off.

#### Automatic reviews

With `[reviews] auto = true` (Settings › Automation), an agent reviews each open PR that asks for your
review, isn't yours and isn't a draft, in a session of its own on `[reviews] account` (an account or
routing group) and `model` (else the account Auto would start, on its own model), apart from the
account new sessions start on. With `[reviews] other_harness = true` (the default), Auto or a
routing group prefers a harness other than the one that wrote the PR (read from a session on this
Mac, or from `Co-Authored-By:` and `Generated with` lines in the PR's head commit or description)
whenever another harness in the pool can review, and passes over any account that has reached its
`usage_cap`. It runs where `[reviews] start` says (Settings › Automation, Where reviews run), which by
default (`"no-terminal"`) never opens a terminal: under Claude Code's background service, else in
Codealong through the account's Agent Client Protocol adapter (Codex, OpenCode), so an account that
can do neither (Antigravity, Cursor) doesn't review. `"codealong"` runs every reviewer in Codealong,
Claude Code too; `"background"` goes as the account's Background does, which for Antigravity and
Cursor is a terminal opened behind your windows; `"terminal"` opens each in your terminal, a tab
or a window as `[apps] new_terminal` says, which comes forward. Reviews of your own PRs and Review with… go there too, in every repo. It
starts in the worktree that holds the PR's branch, else the repo's
main checkout, and is handed the PR and its whole diff as files, each line of the diff numbered by
its place in the file's new version (so it never stops on a permission prompt to ask `gh`, and can
name its lines). What it's told to do is `[reviews] prompt`, which Settings shows to edit, filled
with Codealong's own instructions until you change it. Those ask for bugs, security and data risks,
missing tests and anything that doesn't do what the PR says, and for where the changes don't fit
the code around them in a way that will cost whoever works on it next: put somewhere the codebase
doesn't keep that kind of thing, unlike the way it does things or what the repo's written rules (a
CLAUDE.md, AGENTS.md or contributing guide) ask for, a second copy of something it has, more than
the change needs; differences of taste no rule settles are left out. Yours says what to look for and how,
and, if you like,
to start subagents of its own (in Claude Code, ask for the Task tool and name each one's model) and
combine what they find. Either way Codealong tells it which PR, where it's written out, to change
and post nothing, and how to lay out its last message: a summary, then its comments on particular
lines. The session is linked to the PR, and is a helper of it (see *Helpers*): out of Sessions,
the bar and the queues while it doesn't need you, its turns never Your turn.

Review with… on a PR (its panel's header, its menu, its card's strip, ⌃⌘R) starts the same review
by hand, whatever `[reviews] auto` says: first the reviewer Settings names (Auto's account when it
names none), then each routing group and account that can run one where reviews run, saying where. It's a round like
any other, on the PR's card and in its Reviews tab, its review posted where `[reviews] post` says and
submitted as `[reviews] submit` does; it reviews a commit reviewed already too, a second review going
into your draft after the first. That account's session for the PR, when it has one, is asked again
unless the account has since hit its usage cap, in which case a roomy reviewer takes over.
One that can't start says why where you asked for it. So Settings › Automation shows the reviewer,
where reviews go and what's submitted with automatic reviews off as well. The Codex app's own review
page is offered last, apart from Codealong's.

When its turn ends, its review goes where `[reviews] post` says. By default (`"draft"`) it's a
review on your GitHub account left as a draft that only you see, each comment on its line where the
PR's diff shows it (the rest written into the text, each under a link to its line), added after
what you wrote when you have a draft on that PR already: you finish it in the PR's Reviews tab or on
GitHub, approving or asking for changes, and the review stays asked of you until you submit it. `"comment"` posts a
comment from your account instead, each line comment under a link to its line, and `"none"` leaves
the review in the session. Never a submitted review, which would settle the request to you. What
goes on GitHub says an agent wrote it and which commit it read. When new commits come in while the
PR still waits on you, the same session reviews it again once the branch has held still five
minutes, told what changed since and asked what of its findings is fixed (a session that isn't
running goes on where `[reviews] start` lets it, else a new one starts). A review whose turn is cut
off partway (see *Cut off*: Codealong restarting cuts off the reviews it runs) is told to carry on
the same way, four times at most before its round is given up on; one you stop ends its round
without a review. A start that couldn't reach GitHub to read the PR (the network still coming back
after a wake, GitHub struggling) is tried again by itself, waiting longer each time, for about a
quarter of an hour; a post that doesn't go through is tried a few times too. A round that still
didn't get done says so on the line over the PR's box, "didn't start" when no session ever began
and "didn't finish" when one did, why on hover, and isn't tried again for that commit by itself:
Try again beside it, and in its Reviews tab, does. It posts the review the agent wrote when only
posting failed, and otherwise reviews the PR again as Review with… does on the reviewer Settings
names, in its session for the PR while that can go on. Two run at once at most;
a session waiting on you holds no place. Until the agent's review of the PR's latest commit is done,
or didn't get done (two hours at most), the PR is out of what waits on you (the floating bar, the
notch, the waiting window and the mark), as a snoozed review is, so you review it after the agent;
it stays under Review requested, saying Agent reviewing, and its Reviews tab has a dot. The PR's
Reviews tab, in its panel and in its card in the notch and the waiting window, says where its latest
round stands, with the session that wrote it a click away, and shows your draft as GitHub has it
(see *Your review*); without a draft (a comment, or one you submitted) it shows the review the agent
wrote: its text, then each comment on a line of the code with the lines around it from the PR's
diff, its file and line opening that line on GitHub at the commit it read, and the reviews of
earlier commits too: the latest three a click away, each with how long ago it was reviewed, and
a +N for the rest. Once you've submitted the agent's draft, it says so and how. `codealong
--snapshot --review-plan <owner/name> <number>` prints what a review would start, without starting
it, and `codealong --snapshot --pr-draft <owner/name> <number>` your draft as the Reviews tab reads
it.

#### Reviews of your own pull requests

With `[reviews.own] auto = true` (Settings › Automation, Review the pull requests your sessions
open), each open PR of yours that a session here opened (its primary, see *Nudging a PR's primary
session*) is reviewed the same way, by an agent on `[reviews.own] account` (an account or routing
group) and `model`, else on the account Auto would start, preferring another harness than the PR's
when `[reviews.own] other_harness = true` (the default), so another model reads what one wrote
(Codex reviews what Claude Code wrote, say); one of the same harness only when no other can start. Drafts wait until they're ready for review unless `[reviews.own] drafts` (Review
your draft pull requests too) says to review them as well; a draft of someone else's that asks for
your review is never reviewed. Nor is a PR whose session is gone or archived. Its
instructions are `[reviews.own] prompt`, else Codealong's own; it's told the review goes back to the
agent that wrote the PR, and to end on whether it would approve it (see *Your review*). Nothing goes
on GitHub: the review is kept in Codealong, in the PR's Reviews tab, its comments on lines with the
code around them. With `[reviews.own] send` (on unless turned off), the session that opened the PR
is sent a message once it runs here and doesn't wait on you to answer something: which agent
reviewed which commit, that its review is in a file it reads (kept as an attachment, so another
agent's words never reach it typed in as yours), to fix what holds up and say what it leaves, and
that it isn't asked to merge. A review that found nothing to raise isn't sent, nor one of a commit
the PR has moved on from: the next round's is. A send that doesn't go is tried a few times and then
said. The Reviews tab says where each review went or why it hasn't, and Send hands the latest to the
session by hand while it runs here, whatever `send` says. New commits are reviewed again once the
branch holds still, as above, so a session that fixes what was raised has its fixes looked at.

#### Your review

A PR's Reviews tab starts with your review of it, over everyone else's (under Your review, or The
agent's review for a PR of yours). A draft review of yours on GitHub, the agent's or one you
began there, shows as it will go: its text, then each of its comments with its file and line, the
lines around it from the PR's diff, and Outdated on one whose line has changed since. Edit text and
a comment's Edit change them on GitHub at once (⌘⏎ saves, Escape leaves it), and Remove takes a
comment out on a second click; only you see any of it until it's submitted. A verdict submits the
draft, its comments and all, as GitHub's Finish your review does, from the box at the foot of the
PR's panel or card (Request changes, Comment, Approve; in the panel ⌘⏎ approves and ⌥⌘⏎ comments
from the box, in a card its keys work anywhere). What you write there goes before the draft's text,
and with nothing written the draft goes as it is; a comment or a request for changes needs something
to say, which the draft's text or comments are. Approve in the panel's row of buttons, in a PR's
menu and on PRs picked together submits your draft the same way, as GitHub's own page does, rather
than leave it behind a review of its own. The button's tooltip, the line under a card's box and the
toast after say when a draft goes with it. Without a draft, the box gives a review of its own. The
box stays at the foot of the panel and the card while the details scroll over it, with Review and
Ask (see *Waiting on you*) at its foot, and over it, always, a line saying where the PR's review
stands: the agent's round, your draft and its comments, or that there's none yet; a click on it
shows the Reviews tab. Every agent review ends on what the agent would do with the PR, a `Verdict:`
line (approve, request changes or comment) that isn't posted: the line says what it suggests, and
its button in the box is marked with sparkles. When it suggests requesting changes or commenting,
and its review of the commit the PR's at hasn't been submitted, that button is the filled one and
submits the agent's review as your review in one press, with nothing written: your draft review,
its comments and all, else the review itself (or, posted as a comment, a link to it), with what
you wrote there going first, even into a draft with words of yours. Approve steps back beside it,
and ⌘⏎ still approves; ⌥⌘⏎ comments with the agent's review when it suggests commenting.
`[reviews] submit` (Settings › Automation, Submit the
agent's review for you) has Codealong submit that verdict as your review once the review is posted,
for a PR that waits on your review: `"never"` (the default) leaves every one to you; `"approve"`
approves a PR the agent would approve; `"approve-or-request-changes"` does that and requests changes
where the agent would ask for changes, a true request for changes that can block its merge;
`"approve-or-comment"` sends that request as a comment instead, which reaches the author without
blocking anything. `"request-changes"` and `"comment"` do the same where the agent would ask for
changes, but never approve: a PR it would approve waits on you, for you to approve yourself.
When the agent can't tell (`Verdict: comment`), it's left to you whatever the
setting. Your draft is what's submitted, its comments and all, and the PR leaves what waits on you;
it's held out of it until then, and its card says what was submitted for you. Without a draft, a
request for changes or a comment links to the agent's review posted as a comment, or with posting
off carries the review itself; one whose draft went (submitted or thrown away) isn't submitted. Not
when your draft holds words of yours (the part of its text that isn't the agent's), which is said
instead; a draft of only your own line comments isn't told apart. It never merges. A config.toml
from before, with `auto_approve = true`, reads as `submit = "approve"`. A review of a PR of yours
whose agent would approve it isn't handed back to its session.

#### Settings for one repo, and its team's

Settings › Automation has a picker at its top: Everywhere, or one of the repos checked out here
(also those you have settings for, and those with team settings), each saying when it has team
settings and when you have your own for it. A worktree's menu has Automation for its repo, which
opens Settings there. With a repo picked, the review and nudge settings show what goes for that
repo's pull requests, each saying where its value comes from: Same as everywhere, Team, or Yours
here, with Reset to go back. A change is yours for that repo, kept in config.toml as
`[repos."github.com/owner/name"]` with its own `reviews`, `reviews.own` and `pull_requests` tables
(the reviewer's account and model, its instructions, where reviews go, what's submitted for you,
nudges), each value there over `[reviews]` and `[pull_requests]` and over the team's. Usage limits
and lost connections go the same in every repo.

A repo shares settings with everyone who works on it in `.codealong/config.toml`, committed to it.
Codealong reads it from the branch a pull request merges into, as this Mac last fetched it
(`origin/<branch>`), never from a worktree: a review runs in the worktree that holds the PR's
branch, and a PR mustn't write its own reviewer's instructions or turn on what goes for it. So a
change to the file goes for the team once it's merged; `codealong config check --repo <folder>`
checks the file as a checkout has it, before you push. It's read again when a checkout's refs move
(a fetch) and every few minutes, and a branch a PR newly merges into is read at once; until a PR's
branch has been read (at launch, say), its review and what's submitted for you wait for it, so
nothing the team sets is missed. A link in `.codealong/` isn't followed, and the file goes unused. What it sets goes for everyone: `[reviews] instructions` (or
`instructions_file`, a Markdown file in `.codealong/`), added after each person's own instructions
for reviews of PRs that wait on their review, and the same under `[reviews.own]` for reviews of
their own PRs, the reviewer told it's the team's; `[reviews] never_submit`, the verdicts
(`"approve"`, `"request-changes"`, `"comment"`) never submitted as anyone's review, whatever their
`submit` says; and `[pull_requests] nudge_about`. What spends a person's usage or posts as them
the team only suggests: `[reviews] auto` and `post`, `[reviews.own] auto` and `send`, and
`[pull_requests] nudge_by_default`. Settings, with the repo picked, lists the suggestions you don't
have yet with Use them, which writes them into your table for the repo; and the first PR of the
repo whose Reviews tab has nothing to show says them there once, with Use them and a cross that
dismisses them for good (until the team comes to suggest something else). Accounts, models, a
person's own prompt and what's submitted as them stay theirs: a file that sets one of them, or a
value that isn't one, goes unused, and a notice says why; a key this Codealong doesn't know (a
newer one's) is passed over and said, so teammates on other versions still read the rest.

#### Pins, approvals and snoozes

The PRs you pinned come first, in your order (`p`, Pin PR in its menu, or drag its row up to the
top; drag a pinned one down to unpin it, or ⌃⌘↑ ⌃⌘↓ to reorder), whatever the age filter says. Then
reviews asked of you; they're listed for every repo a project has here, one checked out only on its
default branch too, whose other PRs aren't read. Approve is in a PR's panel and its menu, not on its
row, for any open PR but your own (GitHub doesn't let you approve that), your review asked for or
not; it's filled when it was asked. Snooze review (in the menu, or Snooze… in the panel) puts one
off for 15 minutes, an hour, three hours or until 9 the next morning: it leaves Review requested,
the bar's reviews pill, the mark and the sidebar's count, and waits under Snoozed with when it's
back, the soonest first, until its time or Wake now. PRs picked together open on GitHub one after
another (⌘O), pin (`p`) and copy their links and branch names; the open ones that aren't yours are
approved one after another with one toast, and the ones that ask for your review snoozed together.

#### Merging

A PR's panel, and its card in the notch and the waiting window, has GitHub's merge box where Approve
is, in the row of buttons under its title, right after Approve: what you may do now, and under the
row where it stands for merging when it isn't plainly ready (blocked and why, behind its base,
conflicts, a draft, in the merge queue, auto-merge on). Merge it with a merge commit, squashed or
rebased (picked from the button's caret and kept per repo, GitHub's pick for you until then), the
commit's title and message first, GitHub's own for that way to start from, with the email it goes
under and whether the branch goes too (⌘⏎ confirms); turn auto-merge on or off; add it to the merge
queue or take it out; merge past the rules where you may; bring its branch up to date by a merge or
a rebase; mark a draft ready for review; resolve conflicts on GitHub; delete a merged or closed PR's
branch. A merge names the head commit the box was read at, so GitHub refuses one whose branch has
moved since.

#### Menus, status and details

A PR's right-click menu, on its row here or on its number where a worktree or session shows it (and
the worktree's panel), opens it on GitHub or in Codex and copies its link; a worktree's own menu
copies its PR's link too. Colored pills say where each PR stands: its review, its checks, and
conflicts or an out-of-date branch, or one green Ready to merge when GitHub says it can be merged
now. The review pill says Review requested while anyone's review is asked for (yours or another's;
its tooltip names them), Awaiting review while nobody's is, else Draft, Changes requested,
Approved, or Agent reviewing while Codealong's agent is on it. Rows that are your move are marked at the left edge: amber for a review asked of you or
something to fix, red for failing checks, green for your PRs that are ready to merge. A PR's labels
on GitHub follow its title on its row, each with a dot in its color there and its description on
hover, as many as fit in a little under half the line; all of them show in its header and on a link's
card. The search finds a PR by a label's name. A label added on GitHub shows within ten minutes,
when the repo's PRs are next read in full. Selecting a PR shows a short header: its number and
title, one line of its repo, branch, author and last update, its pills and size, its labels, and
the worktrees and sessions behind it on this Mac as chips (a worktree's
chip, then its sessions', each named by its title with its account on hover, the PR's primary
session marked; a click on a session shows it in the panel, on a worktree shows it in Worktrees,
and a right-click gives either's menu). A PR's row names one of its sessions the same way, its
primary one first, and how many more; a todo's row names up to three. The chevron
beside the title opens its details, and they stay open for the next PR until you close them: the
whole title, its repo, branch, author, when it was opened and its commits, and the chips as a tree.
⇧⌘F (or the button beside the chevron) expands the panel to the whole window, as a session's does
in Sessions, for reading its description and diffs at the window's width. Under the merge box are
its parts as tabs, by whose words they are, which stay at the top as what's under them scrolls
(← → between them): Description, what its author wrote of it; Changes, its changed files with their
diffs (each file's name, then its folder; a filter beside Files changed, which the file count goes
to and which stays under the tabs as the list scrolls, narrows them to the paths holding every word
typed, `-word` leaving some out, then lists the files whose changes mention the words, which are
marked in the paths and diffs; ↑ ↓ and ⏎ open one, Escape clears it, ⌘F in the PR's panel or card
comes back to it); and Reviews, your review first (see *Your review*), then everyone's.

Everyone on the PR is listed once under Reviews and comments: whoever asked for changes first, then
whoever commented, approved or was dismissed, then whoever is asked to review and hasn't, then
whoever only wrote (its author, bots). Each says where their review stands (their last approval or
request for changes, else a comment), Asked again when their review was asked for since, and under
it everything they wrote: their reviews' text, the threads they began on the code, and their
comments on the conversation. What's still open comes first, marked down its left edge: a thread
nobody resolved, and, with `[jev] pull_requests`, a comment asking for a change or an answer. A
thread says its file and line (`line 12 before` for one on the file as it was, a line the PR
removes say, numbered so), has the lines around it from the PR's diff while it's open, and says
how many replies follow; a resolved one is folded to a line. A click on any comment's head opens it
on GitHub, where its replies are. Three show for each person, besides what's open, until Show more,
and a long report (a bot's) shows cut short until Show all. The tab counts your draft's comments
while you have a draft, else what's still open from others. A PR opens on the tab it showed last,
else on Reviews when the agent's review is there to read and the PR waits on yours, or when someone
asked for changes to a PR of yours, else on Description. The tree under On this Mac says whether the
worktree has anything GitHub doesn't, and lists the agents that worked on the PR, including one
that made the worktree from another folder, with each one's title and what it's doing. Clicking one
of those sessions, here or in a todo's panel, shows it in the panel (its conversation, a box to
answer it) with a link back; in the tree, double-click or its arrow opens it where it runs.

#### Diffs

A file's diff numbers its lines before and after. The switches beside the filter draw it one
under the other (Unified) or, where the list is wide enough (the panel expanded to the window, say),
side by side (Split), and wrap long lines or let them scroll sideways; both stay as you left them,
for every PR. As the pointer comes onto a file's row, two buttons show at its end: its diff in your
editor, under the editor's own icon, and the file on GitHub, at its place on Files changed. Its
right-click menu has those, the file at its first change in your editor, and Copy path. Where a
worktree on this Mac holds the PR, a line's number after opens the file at that line.

The diff in your editor is the file where the PR's branch left its base (their merge base, as
GitHub's own diff has it) beside your copy in the worktree that holds the PR, so what you change
there is the real thing and has what you haven't pushed; with no such worktree it's beside the
file at the PR's head. The sides read from GitHub are kept read-only in the system's temp folder
for a week. VS Code, Cursor, Windsurf, Kiro and Zed open diffs this way, through the command line
inside their app: yours (`[apps] editor`) when it's one of them, else the first of them installed.

### Todo

Work you mean to hand an agent. Each todo has a title, a description that can hold screenshots and
any other files (paste, drop or attach them; files copied in the Finder paste too), colored labels,
and a status: Todo, In progress (an agent is on it) or Done. The list has In progress, In review,
Ready, Waiting and Done. Add one in the box at the top (`n`), reorder by dragging or with ⌃⌘↑ ⌃⌘↓,
mark one done with `x`. A todo under way is In review once its work waits on your verdict: while
its PR is open and not a draft (approved or with changes asked for too), or merged and the todo
not yet marked done, its PR being the one of a worktree linked to it, or of a session's worktree
while the session is on that worktree's branch; or once it's marked ready for review, by you
(`r`, or Ready for review in its menu) or by its agent (`codealong todo review <id>`, which a
session started from a todo is told to run when it hands the work back). A mark holds until the
todo is done, a session starts on it, or you send it back (`r` again, Back to in progress);
marking it waiting ends it too, and a todo not started that's marked goes in progress. The
sidebar's In review lists what's under it, and a todo whose PR is a draft. A todo can wait on
other todos (Waits on, in its panel): until they're done it's
under Waiting and shows what it's waiting on, and a dependency that would make a loop is refused.
One an agent is already on stays In progress. Mark a todo as waiting by hand (`w`, the right-click
menu, or type what it waits for in Waits on) when it waits on something outside the list, like a
person's answer or a release; it stays there until you stop it waiting, start it or finish it. Stop
waiting (`w` again) also drops what it waits on that isn't done, so it's Ready at once; the toast
can undo it. Start session (⌘N) opens the quick panel with the todo as the prompt, and the session
it starts is linked to the todo, which moves to In progress; one started in the Claude app is linked
when it shows up. ⌘-click (or ⇧-click, ⇧↑ ⇧↓) picks several todos: Start one session (⌘N) starts a
single session on a prompt that numbers each of them, linked to all of them, and `x`, `w`, `r` and
Delete act on all of them.

### Kanban

The project's todos and sessions as cards in four columns, by how far the work has got: To do,
In progress, In review and Done. A card is a todo, with the sessions on it listed under its two
lines, the most urgent first, or a session on no open todo the board shows, while it's running or
waits on you, or its PR is in review: one that ended and that you've seen leaves the board, as do
helpers and headless runs that don't need you, which work for another session. Its face is the one cards on
the canvas and the desktop have. A todo is in the column its Todo section says (Ready and Waiting
are To do, with a line where Waiting begins); a session is In review while the PR it worked on is
open and not a draft, Done once that PR merged or its turn ended saying its work landed, unless
it's working or waits on you, and In progress otherwise. A card moves only when its work does
(started, a PR opened, merged, done), never with each turn: whose move it is shows in its stripe,
amber when it needs you, and in its place. To do keeps your order (drag, or ⌃⌘↑ ⌃⌘↓); In
progress and In review list what needs you first, the longest wait first, then your turns, then
what's working, then the rest, the one you last sent a prompt first; Done, the latest first, is
folded to a rail with its count until you open it. Done's todos go back as far as the age filter
does (two weeks under All), which narrows the sessions too; the list's find, the statuses and
the devices narrow it as they do the other views, and a label shows only todos.

Select a card for its panel: a todo's, or a session's with its conversation and its box. The
arrows go up and down a column and across to the next one that has cards; Return opens a card
where it lives, and a card has its row's menu, letters and chords (`x`, `w`, `r` and `d` on a
todo, the session's on a session, ⇧⌘E Mark as done). ⌘-click and ⇧-click pick several todos. `n`
adds a todo in the box at the top of To do. Dragged to another column, a card does what the
column stands for, which the drag says as it goes: a todo goes back to To do, starts a session
(the quick panel, as ⌘N does) from To do to In progress, is marked ready for review, is sent back
from In review to In progress, or is marked done, and the toast can undo what it did; a session
dropped on Done is marked done as its menu marks it, and one dropped on a todo's card joins that
todo. A todo In review because of its PR stays there, and a session's PR keeps it where it is.
A card dragged out of the window is pinned to the desktop.

#### Linking

Link worktrees and sessions you already have from the todo, or from a session's or worktree's
right-click menu (Add to todo). A session or worktree on a todo shows the todo's chip; double-click
it (click it in a panel) to go to the todo. The todo's panel lists its worktrees and sessions as a
PR's does: as chips, or, with the chevron beside Link, as a tree with each session's title and what
it's doing, which stays so for the next todo.

#### The primary session

The first session on a todo is its primary session, so two agents don't do the same work: while it
runs, the todo opens it, its panel offers Show session before Start another, and a session started
for it works alongside it and is told to send it what it finds. Once it stops, the next session on
the todo takes over and is told where the last one's records are. A session's right-click menu makes
it a todo's primary session (Primary session for …), and Move to hands the role to the copy. An
agent's `codealong todo start` is turned down while another session is the primary one and runs,
naming it; `--take` takes the todo over.

#### Jev and todos

With Jev on for todos (Settings › Jev, `[jev] todos`), a session that starts outside a todo is
linked to the open todo it's for when Jev is sure; a todo whose linked sessions ended their turns,
or whose PR merged, shows Looks done when Jev reads it as finished (Mark done, or Not yet until
something moves on); and a todo's panel suggests its project, labels and the todos it waits on as
chips you click, reading the screenshots in the todo's description too. Nothing is asked twice:
what was asked is kept in `jev-todos.json`.

### Canvas

The project's work laid out in space, to remember by where it is. Worktrees, sessions, todos and PRs
are compact cards (status, title, where it stands); rest the pointer on one for a popover with the
rest (move onto it to select its text, click its PR chip to select the PR's card, or show the PR in
PRs when the canvas has no card of it, and double-click the chip to open it on GitHub, click a
session in it to go
to that session, right-click one for its menu), select one for its panel on the right (a session's
shows its whole conversation and takes messages; the rest are the ones their own views show), find
(⌘F) or pick a status in the sidebar to fade what doesn't match (a status, or Return in the find,
also zooms to what does), double-click to open it where it lives, right-click
for its usual menu.

#### What's on it

What's on it is set in two places. Tidy (its panel, or Tidy up in a right-click) lays the canvas out
once, holding what its own settings say, each kind on or off: sessions active in the last 1h to 7d
or all (24h at first) and any that's working or needs you; open todos changed or worked on in the
last 7 or 30 days, or all of them; recent worktrees even with nothing on them (off, a worktree comes
on only to head what's there); and PRs, yours and ones waiting on your review, or every open one. It
lays the cards out in columns, one per worktree, in a frame per repo or per day, takes off cards
older than that, kept or not, which is how the sessions you never archived leave, and says first
what would come off and on. Frames you drew or renamed stay, with what's in them. Bring in (the gear
in the toolbar) has the same choices for new work: what moves on after the last tidy (a new prompt,
a finished turn, a question for you, a changed todo) comes on by itself and lands in its frame
beside what it belongs to (a session under its worktree, a worktree in the next free column of its
repo's or the latest day's frame, else in one column beside the frames); the frame grows to hold it,
and what's below or beside moves along whole. Nothing comes off by itself, and what you take off
doesn't come back by itself. Add older things from Add in the toolbar, a right-click on the canvas,
or Add to canvas in any row's menu. Notes (`n`) and frames (`f`) organize it; a frame carries what's
in it, and grows to hold a card dropped over its edge. Clear canvas (in Tidy, or a right-click on
the canvas) takes everything off; Bring in adds work back as it moves on, and a tidy brings back
what its settings hold.

#### Arranged by an agent

An agent can lay the canvas out by what the work is for, which Tidy's repos and days can't:
`codealong canvas arrange` takes frames it names (a release, a fix, an investigation), each with
the worktrees, sessions, todos and PRs that belong to it and a note saying where that work stands,
and lays them out in that order, each frame's cards in columns by worktree and its note across its
top. In Claude Code, `/codealong:visualize` has an agent do it for your recent and running
sessions. Frames you drew and notes stay where they are, with what's in them, unless the agent
names a card that's in one; the cards no frame names go below, laid out by worktree, or come off
when it asks. Tidy leaves arranged frames and their notes as if you drew them; the next arrange
replaces them, unless you renamed the frame or edited the note, which makes it yours. With
`--show`, the main window comes forward on the canvas with the new frames in view.
`codealong canvas show` lists a canvas's frames and what's in each. Both need the app running,
and work on any project's canvas, whichever one the main window shows; a second window showing
the same project's canvas follows the change.

#### Controls

Controls follow Figma: scroll or Space-drag to pan, pinch or ⌘-scroll to zoom, ⇧1 fit all, ⇧2 the
selection, ⇧0 100%. Zoomed out, cards become blocks in their status color drawn by the GPU (WebGL2).

### Sidebar

Picks the view (Sessions, Worktrees, PRs, Todo, Kanban, Canvas) and narrows it: to the devices you pick, to
what was active in the last 24h, 3d or 7d, and to the statuses you pick (Needs you, Your turn,
Working, In review, PR needs work; any of them). In Sessions it also sorts, by status or by when
sessions started. Devices and statuses toggle; none picked shows all, and Clear unpicks them. Save
the view as it is under a name to come back to it; right-click a saved view to update or delete it
(they're kept per Mac, not in config.toml). ⌃⌘S, or the button beside the traffic lights, folds it
to a rail of the views and devices whose filter button carries a badge while a filter narrows the
view.

### Find in the list

Finding anything anywhere, and starting work, is the quick panel's (the box in the middle of the
title bar, ⌘K). Finding in what the view lists is ⌘F's from the list, and ⌥⌘F's or `/`'s from
anywhere in the window (a panel taking the whole window gives the list its place back): a find opens in the strip over the list (beside Sessions' sort and rows
buttons), as ⌘F in a session's panel opens one over its conversation, saying the view it finds in.
What's typed narrows the view's list, and on the canvas fades the cards that don't match. ↓ hands
the list the keyboard, ⏎ opens the selected row (on the canvas, zooms to what matches), Escape or
its × clears it and puts it away. Left with nothing typed it goes away by itself; left with words it
stays, the list narrowed, as you change views, and ⌘F gives it the keyboard back with the words
selected. It finds by every word typed, in any order (a word of one
or two letters among others counts only whole, as in the quick panel): in what describes a row (a
session's title, account and latest line, its worktree, branch and PR; a worktree's name, branch,
path and PR; a PR's number, title, author, branch and repo; a todo's title, description, labels and
what it's linked to), or, from three letters on, in what was said in a session (your prompts and its
replies, for sessions active in the last `[search] days`, your other devices' sessions too, each
searched on its own Mac). A session found only by what was said in it shows the line that has the
words under its title, and its worktree, its PRs and its todos are found with it. Selecting it opens
its conversation with the find bar on those words (*Find*). While Codealong is still reading what
recent sessions said, a find in Sessions that finds nothing says so.

### Notices

What stops working in the background (config.toml that doesn't read, GitHub signed out or out of
requests, a folder macOS is asking about) is a chip at the end of the accounts bar, in a few words.
A click shows the whole of it, when it ends where that's known (GitHub's rate limit says until
when), and what to open to fix it; its button dismisses it until it happens again. Several are one
chip that says how many. An automatic review that couldn't start, or whose review couldn't be
posted, has Try again on its notice, as on its PR's card (see *Automatic reviews*); while a start
that couldn't reach GitHub is still being tried again by itself, the notice says so. The notice goes
once a review starts.

### Tips

While a window waits on what it reads, a tip on using Codealong shows under what it's reading: the
main window before its first read of your sessions, the PRs view until its PRs come from GitHub,
the waiting window and the quick panel at launch, a conversation or a PR's description that takes
a moment to come. Where the whole window or pane is waiting (the main window, the waiting window,
a conversation), the Codealong mark and what's being read sit in the middle of it with the tip
under them; the main window gives the board the panel's room too until it has read your sessions,
and the PRs view until it has its PRs, since the panel has nothing to show before then. In the
quick panel and a PR's description the tip is a short note under the line that says what's
loading. Each place has tips of its own (the waiting window's are about its keys, the quick panel's about its own), with the keys
as they are now and the shortcuts as Settings has them; a tip about a shortcut that's off doesn't
show. At launch the tip comes with the line over it; a conversation or a description gets one only
once it has taken a moment, since they often come at once. Another comes every 12 seconds while the
wait goes on, and Next tip brings one at once. They come in turn, the ones you haven't seen first,
wherever you saw the last. A conversation whose agent works gets one now and then too, under the
line that says so (*While it works*).

### Light or dark

Follows macOS unless you pick light or dark with the buttons at the foot of the sidebar. The choice
is saved in config.toml, and every window follows it.

### Zoom

⌘= (or ⌘+) makes the main window and the project windows bigger, ⌘- smaller and ⌘0 puts them back,
as in VS Code and a browser, from anywhere in them, a text field too; View has the same as Zoom in,
Zoom out and Actual size. It goes in steps from 50% to 200%, is saved in config.toml (`[appearance]
zoom`), and every board window follows it. In the Canvas view the keys zoom the canvas, unless
you're typing in it. The session or PR shown in the notch, the peek, a kept session and the waiting
window draws at the same zoom, and the keys work there too; the notch and the peek open as much
bigger, while the notch's shape and the bar's pills stay at actual size.

### Resizable panels

Drag the inner edge of the sidebar or the panel on the right, and double-click it to go back to the
default width. The PR panel keeps a width of its own.

### New window

*File › New window, ⌘⇧N*

One more window on the project you're looking at. Closing the main window hides it; New window, the
Dock icon, the bar or ⌥⌘; brings it back, and ⌘Q quits.

### Back to Codealong's window

*⌥⌘; from any app, `[shortcuts] main_window`*

Brings forward the window you were in last, the main window or a project's (the main window when
that one's been closed since), out of the Dock if it's minimized. Pressed while you're in one of
them, it hides Codealong as ⌘H does, and macOS goes back to the app you were in; the floating bar,
the quick panel and the cards pinned to the desktop stay. Settings › General changes it or turns it
off.

### Settings window

*⌘, (comma), the app menu, or the foot of the sidebar*

A form for everything in config.toml. General has the folders searched for repos, the shortcuts
that work in any app, the terminal and editor, the theme, and Open at login (off until you turn it
on; the system keeps it rather than config.toml: on macOS System Settings › General › Login Items
lists Codealong, and on Linux it's an entry in `~/.config/autostart`, and turning it off there
shows here); Sounds has the sound for each thing that starts to
wait on you, and how loud (see *Sounds*); Jev has the TypeSafe key (kept in the keychain),
what Jev's requests used and cost (see *What sessions used and cost*), and a switch per feature,
each saying what it sends; Accounts adds, edits, turns off, removes and
signs in accounts, says how each pays (a plan's price a month, or by the token; see *What sessions
used and cost*), and under Found on this Mac lists the ones Codealong could read but doesn't, to add
with a click (with the agent skill, unless you untick it); Routing has Auto's rule, routing groups and Jev's routing instructions; Automation has what Codealong does by itself:
the agent that reviews the pull requests that wait on your review (the account and model it runs on
and what it's told, in a box like a session's, and where its review goes), nudging the session that
opened a pull request, moving a session that hits its usage limit, and telling one a lost connection
stopped to carry on, everywhere or for one repo with its team's settings (see *Settings for one
repo, and its team's*); Projects groups repos by pattern; Advanced turns
the performance log on and shows where the files are. Each change is saved in its place in the file,
so comments and order stay as you wrote them. The config.toml section edits the file itself, for
anything the form doesn't cover, and says what's wrong when it doesn't parse.

An account's card in Settings › Accounts says who it's signed in as. OpenCode signs in to the
providers you bring rather than to an account of its own, so its card names them (*Signed in to
Anthropic and GitHub Copilot*), or says *Free models only* when it has none: OpenCode's own free
models need no sign-in. **Turn off** keeps an account in config.toml (`disabled = true`) and has
Codealong start nothing on it: no new session, resume, fork or move, no review, helper or question
about a pull request, and no pick of it by Auto or in the quick panel. Its usage isn't read, and it
leaves the usage bar and Settings › Agents. Its sessions still show on the board, and running ones
still take what you send them. **Turn on** brings it back; **Remove** stops reading it at all.

## Finding and starting work

### Quick panel

*⌥⌘/ from any app, over full-screen ones too, or ⌥⌘' set to a chat; ⌘K, ⌘N, the box in the middle
of the title bar (which says the panel's shortcut) or its +, the inspector, or a worktree's or
session's right-click menu in Codealong*

One input, and below it what you can do next: it finds and starts, and answering is the waiting
window's (below). With nothing typed it lists the sessions that need you, the longest wait first as
on the floating bar, then the PRs waiting on your review under Review requested, the longest wait
first, then the sessions whose turn ended (the working ones when nothing waits on you), and under
them the first three todos ready to start, in your order, then the rest of the sessions started in
the last day. A snoozed session doesn't wait on you there until its time: it's listed with the rest,
and a search finds it among the idle ones, with a chip saying how long until it's back. Typed words find sessions by title,
account, worktree, branch, repo, project or PR, then by what was said in them (your prompts and the
agents' replies, for sessions active in the last `[search] days`, with the matching line shown; the
words as you typed them rank first), six at first and the rest a row away, and under them todos by
title, description, label, project, or the worktrees and sessions linked to them, then PRs by
number, title, repo, branch or author. A word of one or two letters among others counts only as a
whole word, so "class c" doesn't find everything with a c in it. Archived sessions are left out
unless you ask for them: the Archived box at the end of the Sessions heading, there whenever you
search, says how many match, and checked (a click, or ⇧⌘A) lists them after the others, each with an
Archived chip, in this and later searches until you uncheck it. ⏎ on a session shows it in
Codealong, with its conversation and a box to answer it (one found by what was said, with the find
bar on the words), and ⌥⏎ opens it where it lives. ⏎ on a todo
shows its session the same way, or, when it has none, gets a session ready to start for it (its
prompt, its worktree, linked to it); on a PR it shows the PR in Codealong, and ⌥⏎ opens it on
GitHub; the PR's right-click menu copies its link. ⏎ or ⇥ on a PR under Review requested reviews it
in the waiting window, and ⇥ on a session answers it there; the panel puts itself away. The last row
starts a new session with what you typed as its prompt. ⏎ does the selected row, ⌘⏎ always starts,
⌥⏎ shows a todo or a PR under Review requested in Codealong, ⌘1 to ⌘9 do a row, Escape closes. Each
open starts empty, so ⏎ goes to what needs you; ↑ brings back what you'd typed before closing (kept
as you type, so a relaunch doesn't lose it), then earlier prompts you sent, started or saved, and ↓
goes forward again. The panel keeps one size whatever it lists (smaller only on a screen too short
for it): the list scrolls within it, the row that starts a new session stays above the bar, and the
bar's menus open over the list. A click does a row
only when you moved the pointer onto it: one the list moved under a pointer that stayed put (as
suggestions and what was said come in) is selected by the click, and a second click does it. A
project picker (⌘P) narrows everything to one project. ⌥⌘' (`[shortcuts] chat`) opens the same
panel with the worktree picker on No project, so what you type starts a chat of its own, in a folder
of its own among Codealong's chats, rather than in a worktree; it finds sessions as ⌥⌘/ does, and
the picker (⌘T) still goes to a worktree. The shortcuts are yours to change in Settings › General;
Settings names shortcuts macOS and common launchers already use.

A prompt (a few words, not a word or two of a search) can go to any session, not only one Jev
suggests: Send to another session…, the row under Jev's suggestions (at the end of Sessions with
suggestions off), or ⌘F, empties the box to find the session it goes to, and what you wrote waits under it, as written. With nothing typed there
it lists every session in the project, the ones Jev weighed first, in its order and with how likely
each is, then the rest, the one you last prompted first, so the list holds still while agents work;
words typed there find sessions as a search does, by what was said in them too, Jev's first.
Helpers and headless runs are left out while they don't need you, and archived sessions unless the
Archived box (⇧⌘A) asks for them. It lists 40 at most and says how many more there are. ⏎ sends the
prompt to the selected session as a suggestion would: a session waiting on you to allow something is
shown in Codealong to answer there instead, and one that isn't running goes on with it, as the
row's tooltip says. ⌥⏎ shows the session in Codealong instead, keeping your prompt (one waiting on
you opens where it lives), ⇥ answers it in the waiting window, and its right-click menu sends the
prompt too. Escape, ⌫ in the empty box, ⌘F again or a click on the
prompt puts the prompt back in the box. The new session's row and the bar under it, which only set
up a new session, are put away meanwhile.

### Suggestions in the quick panel

*Optional; Settings › Jev, `[jev] quick_panel` in config.toml*

Once you pause typing, TypeSafe's Jev reads what you wrote against the project's sessions and
worktrees and the panel leads with up to five suggestions, each with how likely it is: send it to a
session that's already going, start new work in a worktree (a new one when it's a change of its own)
or, for a question that needs none of your repos, in a chat of its own, open a session, or keep it
as a todo. The rows the likeliest leave go to more of the sessions it weighed, in its order, while
each is at least 5% likely to be the one meant. ⏎ does the first. A start that would start just
what the new session's row at the foot does (the same worktree, account, model, effort and place)
isn't listed again: that row says how likely Jev thinks it is, and ⏎ starts it when it's the
likeliest. Jev never changes the bar's menus; they stay as you set them. The same call says whether
a running session may already be doing it (start rows say so, and sending to it is offered), and
puts the sessions it thinks you mean first among the search results and in Send to, which lists
every one it weighed (up to 20), not only the ones it suggests. A session that's running leads one that isn't when Jev finds them close (a
stopped one counts half against running ones), since sending to it doesn't resume anything.
Suggested is there from the moment what you type goes to Jev (a few words, not a word or two of a
search), saying it's working until Jev answers. At the next key the last answer's suggestions stay
where they were, dimmed and without keys, doing nothing until Jev answers again, and the group only
grows while you type, so the rows under it hold still. With `[routing] prompt` set (Settings ›
Routing), Jev also suggests an agent, model or routing group for a new session, resolved to an
account with room; without one, a new session starts on the one the start menu shows. The API key stays in
the login keychain. What you type, repo names and session titles and latest lines go to TypeSafe.
Images you paste go too, each where it is in the text, so a screenshot can say which app or
session you mean: up to four, scaled down to what Jev takes (padded out when they're tiny). When
TypeSafe won't take one, Jev answers from the words alone.

#### How a prompt gets to a session

Sending goes the way each harness takes a prompt: pasted into the session's Ghostty, iTerm2,
Terminal or tmux tab, Claude Code's daemon for a background job, OpenCode's service, a turn started
by the Codex app itself (opening the thread there first when no window has it; when the app doesn't
take it, nothing is sent: the message goes back in the box with why, and the app opens the thread
with it filled in to send from there), and Claude Code's cross-session socket for Claude app
sessions, which read it as a message from another session (what you type in Codealong says it's
yours, so one that skips permission prompts takes it without asking, and that its answer goes in
its reply, where you read it, not in a message back). A terminal session whose tab
can't be found gets such a message too, and the note (on the bar, from the peek) says it wasn't
typed and why; once typing has started, nothing else is tried, so it can't arrive twice, and the
same text sent again while it's on its way goes once.

Ghostty is found and typed into by asking it, and macOS naps Ghostty while its windows are behind
other apps' (App Nap): napped on a busy Mac it took seconds to answer, so a reply went as a message,
an approval or an open failed, until you brought Ghostty forward. Codealong waits for its first
answer before it starts timing, and when Ghostty doesn't answer in time and macOS may nap it, the
note says so. `defaults write com.mitchellh.ghostty NSAppSleepDisabled -bool YES` keeps it awake
once Ghostty restarts.

#### What Codealong sends by itself

What Codealong sends a session by itself, as you set it up to (a PR's news to its primary session, a
review handed back to the session that opened a PR, a helper's news, carrying on after a lost
connection, a conversation moved to another account at a usage limit, a review session told of new
commits), never goes as yours. It starts with a line saying Codealong sent it by itself, that its
user didn't write it, and that no session did either, so what the agent has to say about it goes in
its reply; it goes the way another session's message does where the harness has one:
Claude Code gets it on its cross-session socket under the name Codealong, so the agent reads it as a
message from Codealong and not as a prompt, and it isn't held for you, since your setting sent it. A
harness without such a way gets it typed in, that line first. Either way its conversation shows it
as Codealong's, apart from yours and the agent's (a rule at its left), on the phone as a note.
`codealong --snapshot --tell <key> <text>` sends one the same way.

#### Sessions that aren't running

A session that isn't running is resumed with the prompt where the place chip beside Send says (a
menu, with a line on each), among the places its account starts sessions in that its harness can go
on in: a terminal, the background (Claude Code's background service, else a terminal behind your
windows), its app, or Codealong, which runs it from then on. Beside Send, where a running session's
box says its model, is the account's model and effort menu, as the start menu has it, and picking a
model leaves only the places that take one. It goes on where the account's last one did, else in a
terminal; Resume in, in its right-click menu, has the same places, to go on without a prompt. One
waiting on you is opened instead, so what you typed can't answer its question.

#### Switching models and effort

A running session's box says the model it's on beside Send (the one its latest reply came from, or
that it was switched to since), and after it, quieter, the effort it's on, for Claude Code and Codex
sessions, as the harness writes the level (`low`, `medium`, `high`, `xhigh`, `max`; Codex's
`minimal` and `none` too): the level its latest reply was made at, or one `/effort` set since. A
session that isn't running says the effort it goes on with the same way, once one is picked. A
Cursor app chat says the model it's set to by Cursor's name for it (Grok 4.7, Auto) and its effort,
between turns too, since that's what its next turn goes on with; it's switched in Cursor. A thread
in the Codex app says its model and effort between turns too, for the same reason. An OpenCode
session says the model its next turn runs on by OpenCode's id for it (`big-pickle`) and the
reasoning variant it was picked with, else the one its latest reply on that model was made at,
between turns too, since a message Codealong sends goes on with them. An Antigravity
conversation says the model its latest turn ran on, whose name has its level (Gemini 4 Argon
(High)).

Where Codealong types into its terminal or runs it, that chip is a menu of its account's models,
with its effort levels under them, the one it's on checked: picking either switches that session
alone, between turns (`/model <name>` or `/effort <level>` typed into a Claude Code session, its
agent's model or effort for one Codealong runs). Claude Code asks before a switch while the
conversation is cached for what it's on ("Your next response will be slower"); Codealong answers
yes, as picking it meant. It also saves what it's switched to as the account's default for new
sessions; Codealong puts the default back a few seconds later, so the switch stays with that
session, and the box says what happened once that's done. Meanwhile the account's other running
sessions may follow the new default. A Codex session in a terminal is switched there, with its own
`/model`; one Codealong runs is switched from the chip.

A thread in the Codex app is switched from the chip too, running or not: the app takes the model
or level for the thread's next turn, as a pick in its own box does, and its box shows it. A model
keeps the thread's level where the model takes it, else the nearest it does. One no Codex window
has open is opened in the app first, as a send's is. A session on another of your Macs is switched
the same way, from that Mac's accounts' menus, as is the mode of one that Mac runs in Codealong;
the Mac needs remote control on, and a build of Codealong from 2026-10-17 or later for a Codex
app thread.

### New session

**New session**, the panel's last row: pick the worktree (⌘T; it starts at whatever's selected in
Codealong: a worktree, a session's, a PR's, or for a PR with none here its repo's main checkout;
pinned ones first), the account and model in one menu (⌘L, every account on every model it can start
on, with its limits, and the effort under them: Default, Low, Medium, High, Extra or Max, for Codex
Extra high and Ultra too, ← → to change it), and where it runs (⌘O, a chip beside the agent menu that says the place, with a line on
each in its menu). Where a start on this Mac runs becomes the account's `start` in config.toml (with
Auto, of every account Auto picks among), so the next start opens there, and so does `codealong
start` without `--in`; the places keep their order whichever it is. With other Macs linked, ⌘D picks
the Mac too: its worktrees and accounts are listed, and the session starts there (it needs remote
control on over there).

#### Each place

Your terminal (Ghostty, iTerm2 or Terminal) starts Claude Code on your prompt, on the account's
default model or the one you picked (Fable, Opus, Sonnet, Haiku, 1M context, Opus Plan Mode) at the
effort you picked (`--effort`; Haiku takes none, and Default is the account's `effortLevel`), and
with New worktree Claude Code makes a fresh worktree for it (`claude --worktree`: its folder, branch
and base are Claude Code's choice). Codealong trusts the folder for that account first, as Yes at
Claude Code's trust prompt does (the prompt's default answer quits, and `--bg` refuses a folder it
hasn't trusted). Background starts it the same way under Claude Code's background service (`claude
--bg`), with no terminal: it shows on the board like any other, takes prompts from Codealong, and
Attach opens a terminal on it when you want one. The other agents have no background service of
their own, so for them Background opens the terminal behind your windows instead (nothing is asked
to come forward, so you stay where you are). The Claude app opens a new session with the prompt
filled in, and you send it there (its own model and worktree choices included). The Codex app runs
the new thread on your prompt: Codealong makes the thread, opens it in the app and has the app send
the prompt, on the model and effort you picked, else the account's own; without a prompt it opens
the app's new-thread page, where the app picks them. A thread that goes on in the app goes on with
its own, as the app's box would send it. OpenCode starts in your terminal on your prompt
(`--prompt`), and so does the Codex CLI, for any Codex account (`codex -- <prompt>`, images with
`-i`), on one of the models Codex's own `/model` picker lists for the account (`-m`; Default is
`model` in the account's `config.toml`, or its `profile`'s) at the effort you picked
(`model_reasoning_effort`, Low to Ultra; a model that doesn't take the level picked is asked for
the nearest it does, as Codex lists them), and Antigravity's command line, `agy`, once it's
installed (`agy -i <prompt>`, on one of the models it lists); the Antigravity app starts
conversations only in its own window. Cursor's command line, `cursor-agent`, starts in your
terminal on one of the models Cursor's own picker shows, Auto among them (`--model`, each on its
default settings; turn more on in Cursor's Settings › Models and they're listed here too). It keeps
the model it last started on as its default, which is what the account's own line starts on, and
it signs in on its own (`cursor-agent login`), apart from the app. A running one's model, and a
chat's in the Cursor app, are changed in Cursor. Codealong trusts the folder for the Codex CLI and `agy`
first, as it does for Claude Code, since each stops to ask in a folder it hasn't trusted. For Codex
that's an entry in the account's `config.toml` for the repo's main checkout, or for a folder in no
repo (each chat's own); a folder Codex already has an answer for, trusted or not, is left alone.
For `agy` it's its settings' `trustedWorkspaces`, the folder exactly as it's started in; without
it the prompt runs behind the question with no workspace, asking before it reads each file of
yours.

#### In Codealong

**In Codealong** runs the session in Codealong itself, for any Claude Code, Codex or OpenCode
account, through the agent's Agent Client Protocol adapter (`codealong install` adds the ones for
Claude Code and Codex; OpenCode has its own): its panel shows the reply as it's written, takes the
next prompt (queued while a turn runs), answers what it asks to run with Allow and Deny, and Stop
ends a turn, as it does for sessions running elsewhere. The agent keeps the session in its usual
records, so it can be resumed in a terminal too, and after Codealong restarts, sending to it picks
it up again.

#### Auto and routing groups

Auto, the default (and `codealong start`'s), keeps to accounts of the first account's agent while
one has room (or uses Auto's routing group when `[routing] auto` names one), judged by the limits
its default model spends, where `usage_cap` (90% by default, set per account in Settings › Accounts)
of a limit is no room; the others only once they're all
out. Among them it takes the one whose weekly limit resets soonest, so no week's room goes unused,
until that one is within 5% of its cap, that last 5% being left to the sessions already
running on it; else the one with the most room. It starts on that default model, or the next best
(Opus, then Sonnet) when the default's own weekly limit is nearly used up. Named **routing groups**
(`[[routing.groups]]`, Settings › Routing) group `(account, model)` members under a strategy:
`"usage"` (most room / soonest reset), `"alternate-harness"` (rotates across the group's harnesses)
or `"round-robin"`, skipping accounts at their usage cap. A routing group is offered after Auto in
the quick panel, in `codealong start --account <group>`, as the reviewer for PRs, or as Auto's own
rule (`[routing] auto = "<group>"`). Codealong only launches:
the session shows on the board like any other, and the panel stays on whatever you're looking at
when it appears.

#### Files in the prompt

Paste or drop screenshots and other files into the prompt: Claude Code in a terminal gets them as
file mentions (attached when they're 256 KB or less, read otherwise, with their folders added so it
doesn't ask), and the apps and OpenCode get their paths.

#### Chats

**No project**, last in the worktree picker, starts a chat for a question that belongs to none of
your repos: the session gets a folder of its own under `~/Library/Application
Support/Codealong/chats` (trusted once for Claude Code, for all of them, and each one for Codex and
`agy`, which trust a folder only by itself), and no new worktree. The
chats are listed together in one lane, Chats, in Elsewhere, and `codealong start --chat` starts one.

### Improve Codealong

*The bulb at the title bar's right end, beside New session's +; ⌥⌘, from any app (`[shortcuts] improve`, Settings
› General)*

A session that improves Codealong itself, from whatever you just saw. The quick panel opens on
Codealong's own project and its source's main checkout, its input asking how to improve Codealong,
with a picture of every Codealong window that's up (the main window, project windows, the floating
bar or the bar in the notch, the peek, the waiting window, kept sessions, cards on the desktop; the
panel itself aside) as attachments, each named for its window. Describe what to change and start as
usual (the worktree, account, model and place are yours to change): the prompt is what you wrote,
then the pictures, so the agent sees what you saw. A picture you take off the strip stays out. The
pictures are each window's own drawing, asked of WebKit, so macOS asks no screen-recording
permission, and they're kept as attachments are (`images/`).

Codealong's source is any checkout a scan finds whose `src-tauri/tauri.conf.json` is the app's, so
a clone under another name or a fork counts, and its worktrees carry a mark the phone reads too.
When it's in none of your projects the panel says so, with a button that adds a `[[projects]]`
table named Codealong for its repo (or its folder, with no remote); its worktrees come onto the
board, and the panel moves onto them. When another of your Macs has the source and this one
doesn't, the panel goes there, and the pictures go with the prompt. When no checkout is found, it
says to clone github.com/codealong/codealong into a folder Codealong searches. `codealong --snapshot --improve`
prints where a session would go. On the iPhone, the button at the top of the list does the same
with a picture of the phone's screen, kept on the Mac the session starts on.

### Skills by name

Type `/` and the start of a name at the start of what you write: only a skill that starts it is
run, as Claude Code runs one, so a `/name` later on ("the /usage endpoint") is words. In the quick
panel that lists the skills of the project's repos (their `.claude/skills`, `.claude/commands`,
`.agents/skills`, `.codex/skills` and `.cursor/skills`) and your accounts' own, the most run first; ⏎ or ⇥ finishes
the name. A repo's skill picks where the session starts, its repo, in a worktree chosen as new
work's is, with the rest of the line as its prompt, and the skill is written for the agent. A skill several
repos have is a row for each. Only accounts whose agent reads that skill's folder are offered, and
the start row says which are left out. An account's own skill is one row, which names the accounts
that have it; the session starts where the panel is set to, on one of them. In a session's reply box
the list has Codealong's own commands, then the skills of the session's worktree and of its
account: ↑ ↓ choose, ⇥ finishes the name, Escape puts the list away. The name goes as the agent
takes it: `/name` to Claude Code and Cursor, `$name` to Codex. Cursor reads its own skills folders
and Claude Code's, Codex's and the shared `.agents/skills`, in the session's own folder (not the
ones above it) and in your home, so their skills are offered on it too, each by the name of its
folder, as Cursor's command line runs it; only names of letters, digits, `-` and `_` run that way,
and a skill only for Cursor's app isn't offered. A Cursor app chat gets the prompt on the
clipboard, to paste into Cursor.

An account's own skills are its personal ones, its plugins' and the ones claude.ai syncs into it
(for Cursor, the ones it ships too), and for Claude Code also the skills Claude Code has built in: `/design` and `/slides` where the
account may use them, `/loop`, `/code-review`, `/verify`. Its commands, such as `/compact` and
`/model`, aren't skills and aren't listed. Codealong asks each account's Claude Code for those as
it starts, which spends no usage, and again every hour. OpenCode
and Antigravity can't be told to run a skill by name in a prompt, so they're offered none, and
another Mac's skills aren't listed yet.

### Reusable worktrees

A worktree that takes one piece of work after another (its right-click menu's Reuse for new work, or
`codealong reuse on`), where others are kept for one piece of work each. A repo's main worktree
(named `main` on its row, as git calls it) and a worktree that has taken new work three times or
more (another branch checked out, or its branch reset, as its HEAD's reflog says) are reusable
unless turned off. Its row shows a repeat mark before its repo, and Free while it holds no work:
nothing uncommitted or unlanded, and no agent at work (otherwise work under way, in review, or
landed once its PR merged). Jev suggests new work only in reusable worktrees: the repo's free or
landed ones first, joining the work under way in one only when Jev is sure the work belongs there,
else the main worktree. A new worktree is suggested only in a repo where you often start in one from
the quick panel (three or more, and half your latest starts there), and a repo named in what you
type keeps other repos' worktrees out of the suggestions. When a reusable worktree takes new work,
the sessions from before move to Earlier work in its panel, off its row and out of its status.
Settings › Projects lists them, and suggests worktrees whose branch has carried several merged PRs.

## Answering agents from anywhere

### Waiting on you

*⌥⌘. from any app, `[shortcuts] waiting`; Window › Waiting on you; ⇥ on a session in the quick
panel*

A window of its own, sized and placed as you like and kept up until you close it, that works through
what waits on you one at a time: every session that needs you, then the PRs waiting on your review,
then the sessions whose turn ended, each the longest wait first, snoozed ones left out. With the bar
docked in the notch, ⌥⌘. opens the notch instead, as a click on the notch does. Pressed while the
window is the one you're in, ⌥⌘. puts it away and gives the keyboard back to the app you were in. A
session shows as the card the bar's peek shows: the same header (the ways to open the session, the
check, ⋯), the session's whole menu on ⋯ and on a right-click, its conversation, and a box that
sends it a message where it runs (⏎ sends and stays, to watch the reply; ⌘⏎ sends and goes on to the
next), or what it waits on you to allow, with Allow and Deny (⌘⏎ allows, ⌘⌫ denies). A PR shows with
its details and a box that reviews it (⌘⏎ approves, ⌥⌘⏎ comments, Review with… starts Codealong's
review on an account, as below). The box's other side, Ask (or `/side` and a question on Review), puts
questions about the PR to an agent on a Claude Code or Codex account (Auto's, or the one picked
beside Ask): Codealong hands it the PR's description, discussion and whole diff, it runs in a
checkout of the repo here (the PR's own worktree when there is one), and it reads and runs only what
looks. The answers show above the box as a session's side conversation's do, and are kept to read
again (N earlier); neither GitHub nor anyone on the PR sees them, and Review keeps its text
meanwhile. It has no title bar: a strip along the top, with the window's buttons at its start and
that drags the window, has what's commonly done, with its keys (⌘E marks a finished turn seen, ⌃⌘1
to ⌃⌘4 snooze, ⌃⌘A archives, ⌃⌘R renames, ⌃⌘P pins it in Sessions), and says how the last one went
and what the box has to say. Once one is sent a message, allowed, reviewed, marked seen, snoozed or
archived, the next that waits on you takes its place. ⌘] or ⌥⇧↓ skips to the next (a skipped one
comes round again after the rest) and ⌘[ or ⌥⇧↑ goes back, ⌥↑ ⌥↓ step along what waits, and ⌃- ⌃⇧-
go back and forward through the ones you looked at. A message that doesn't go is back in its
session's box, and that session comes round again. With nothing left it says nothing waits on you,
and the next that comes to wait on you shows by itself. Brought up again while it's open, it stays
where you were; closed and brought up again, it starts on the longest wait. Where it was is kept for
the next launch.

### Commands in your terminal

An agent can ask to run a command in a terminal of yours, as you (`codealong terminal run`): a
sign-in that opens a browser or asks for a password (`gcloud auth login`, `aws sso login`), a
command that needs your identity rather than the agent's, a dev server you should see. Its session
waits on you for it, wherever that shows (its panel, the notch, Waiting on you, the bar), under
"Wants to run in your terminal", with the exact command and where it would run. Allow opens a new
tab of your terminal beside the session's, in that folder, and runs it there in your login shell
(zsh or bash, interactive, with your aliases and your sign-ins, outside any sandbox): you can watch
it, type into it (a password prompt, a browser sign-in) and stop it with Ctrl-C. Once it ends the
tab says how, and Return there gives you a shell in it; otherwise the tab closes by itself 10
minutes after (Settings › General › Close a command's tab once it ends; `close_command_tabs_after`
in config.toml's `[apps]`, in minutes, 0 to keep it open), so tabs of finished commands don't pile
up. A tab you took a shell in is yours and stays. What it prints goes back to the agent, colors and
progress lines drawn over themselves resolved; what you type isn't kept, so a password typed at a
prompt that hides it isn't either. Deny tells the agent no, with what you write in the box when you write something. It's asked
every time: there's no Always, and only you allow it, on this Mac or from your phone, never the
agent's command or another Mac. The request goes away if the agent stops waiting for your answer.
You run commands this way yourself too, with `!` in a session's box or Run on a `!` block
(*Replying*): no Allow, since you asked for it, and the session is told what it printed once it ends.
A session that couldn't be told leaves a notice saying so, with where to read what it printed: one
that's gone or waits on you, and one that takes a message only pasted into its app (a Claude app
session that isn't running, Antigravity's app, Cursor's), which isn't brought forward for it.
An agent can read what a run printed while it runs, interrupt it as Ctrl-C would, and close its tab
once it's done, but only while the tab is still the run's: once you've taken a shell in it, it's
yours, and stays.

### Floating bar

A small bar on top of every window, on every Space and over full-screen apps, with a pill for each
agent that needs you, whose turn just ended, or that's running, one for PRs waiting on your review
(its menu lists each with its repo and who asked; a click opens the waiting window on the PR's card,
to review it there), and one for todos ready to start. The project button beside the mark shows one
project or all of them; a badge on it counts what needs you in the others. Rest the pointer on a
session's pill and its peek opens under the bar: the session's conversation, as its panel shows it,
and a box to answer it. The peek never takes the keyboard by itself, so passing over the bar can't
take what you're typing elsewhere; click in it to type, and ⏎ sends where the session runs and puts
it away (Escape does too, keeping what you wrote with the session). A session waiting on you to
allow a tool call shows it with Allow and Deny above the box; its ⋯ button, or a right-click in it,
has the session's menu, as everywhere else. Until you click in it, the peek follows the pointer from
pill to pill and closes once the pointer leaves it; pills that shift under a still pointer as
sessions change groups don't move it. Once you've clicked in it, it stays on its session whatever
the bar does, until you send, press Escape or click somewhere else. Its header has the ways to open
the session, the check that marks it seen, the pin and ⋯; snoozing, archiving, pinning, renaming and
showing it in Codealong are in the ⋯ menu and the right-click menu, and the Sessions list's
shortcuts work on it once it has the keyboard (⌘E seen, ⌃⌘A archive, ⌃⌘R rename, ⌃⌘P pin in
Sessions). The pin **keeps the session on the desktop**: it gets a window of its own where the peek
is, which stays up whatever you do, floating over every window as the cards pinned to the desktop
do. Dragging the peek's right edge, its far edge or the corner between them to make it bigger keeps
it too, at that size, when you let go, and so does dragging it by its header, where you let it go. A
kept session is dragged by its header and sized by the same edges; sending leaves it open; its × (or
⌘W) takes it off the desktop; and it comes back at launch where it was left. A pill whose session is
kept brings that window up in place of a peek. Click a pill to show its session in Codealong,
selected in Sessions, and double-click it to open the session where it lives; drag it off the bar
(floating or in the notch) to pin its card to the desktop where you let go; right-click it for the
session's menu, with Reply here to answer it in the peek and Rename… to rename it there. The todos
pill opens the quick panel ready to start a session for a todo (its menu picks which).

#### Snooze

**Snooze** (a session's menu on the bar or in Codealong, the peek's ⋯ menu, or a folded pill's menu
for all of its sessions): for 15 minutes, an hour, three hours or until 9 the next morning. A
snoozed session leaves Needs you and Your turn (on the bar, in the quick panel and the waiting
window), and the mark stops counting it; the snoozed pill
lists it with when it's back, to show it in Codealong, open it or wake it early. When the time comes
it's back as it is then, and one with nothing new (its turn already seen, or its process gone) comes
back as Your turn, so a snooze always brings its session back. A running session stays in Working
while snoozed, and sending a session a prompt ends its snooze. A session on another device is
snoozed on its Mac, so it's snoozed there too.

#### Quiet

**Quiet** (beside Snooze, in a session's menu anywhere, or for several picked in Sessions) is for a
session that loops or wakes on a schedule: a turn it ends is idle rather than Your turn, so it's in
no queue, on no pill and off the mark, and it waits on you only when it needs you (it asks you
something, waits on a call to allow, or stops with an error). It stays quiet until you uncheck
Quiet, whatever it does meanwhile; Mark as unread, or a snooze that ends, still brings it back. Its
row has a Quiet chip and its header says quiet. A session on another device is quieted on its Mac,
as a snooze is.

#### Suggested replies

An agent that reads your sessions for you can suggest what you might answer each one: start a
session and ask it to go over what waits on you (`codealong sessions --waiting`, then `codealong
session transcript` for each), and it suggests replies with `codealong session suggest`. They show
above the session's box, in every window that has it (Sessions' panel, the peek, the notch, the
waiting window), under who suggested them and the note it left. A click sends one as yours, as it
is, the way ⏎ sends what's typed, leaving what you'd typed in the box; ⌘-click sends it and goes on
to the next that waits on you, as ⌘⏎ does; ⌥-click puts it in the box to change first; ↑ from the
empty box goes through them, ⏎ sending the one you're on. **⌘1** to **⌘6** send them from the
box, as the quick panel's ⌘1 to ⌘9 pick its rows, and from anywhere in the notch and the waiting
window; once ↑ has taken you into them, a number alone does. Each shows its key while it works; from
Sessions' list, where ⌘ and a digit open the row's places, they show their arrows instead. A reply's right-click menu has the same,
and Dismiss takes them all away. The box stays yours to type in instead, and nothing reaches the
session until you pick. While it waits on a call to allow, a reply denies the call and is sent
instead, as what you type would be. For questions it asks you, the agent picks among their own
choices instead (`codealong session suggest --pick "<choice>" "<why>"`): the question box shows
them picked, each with a Suggested chip and its why in a few words, and who picked them and any
note under the choices, so one click on Answer answers with them, and a choice of yours takes a
pick's place (Dismiss takes away the ones you haven't touched). Its row has a
Suggested chip, whose tooltip lists them. They answer the reply the agent read: one
written after you'd replied since is turned down, as is one while the session takes a turn, so a
suggestion for the turn before never appears after your answer. They go once anything is sent to
the session or a turn of its starts, and suggesting again replaces them. A
session on another device keeps them on its Mac.

#### Queues, pills and the mark

Needs you, Your turn and Working are queues, the longest first, so a new one joins the end instead
of pushing an old one down, and the notch works through them first come, first served. Pills that
don't fit fold into one per group that lists them, and a session picked from that list, or from the
recent pill's, shows in Codealong's window. The mark brings Codealong forward; drag the bar by it or
by its gaps. Its node, the part that carries the status color, moves as things happen: it breathes
while agents work, beats as a turn ends (twice as something starts to need you), sends its color
once round the C as a new session starts working, and lets a dot go as you send a session a message
from Codealong. Point at the mark and the C turns once about the node, and turns back as the pointer
leaves. With Reduce Motion on it only changes color. A short sound goes with what starts to wait on
you (*Sounds*).

#### In the notch

**In the notch**, the bar docks at the top of the screen instead: one black shape with the notch,
dark whatever the theme, showing the mark on the camera's left and, on its right, the most urgent
thing and how many (Needs you, then PRs that wait on your review, then Your turn, then Working; a
new one that needs you pulses). Rest the pointer on it for a moment, or click it, and it opens down
into the bar's pills, with the project and every count beside the camera; it closes a moment after
the pointer leaves, and stays open while one of its menus is up. The pin at the end of its right
side, or Keep the notch open in its right-click menu, keeps it open until you unpin it, across
launches. There's no peek beside a docked bar: click a session's pill and the notch opens further,
into that session under the pills, its header, its conversation and the box that answers it, dark as
the notch is; ⌥⌘. from any app opens it on the session that waits on you. The reviews pill opens it
the same way on a PR waiting on your review (the one PR; a menu picks among several, and the count
beside the camera opens the first): the PR's card, with its details and a box at its foot that
reviews it. It has the keyboard from the click, with the cursor in the box, and works through what
waits on you as a session's panel in Sessions does, with the same keys: ⌘⏎ sends (or allows what the
session waits on) and goes on to the next that waits on you, ⌘E marks a finished turn seen and goes
on (from one that runs it goes on all the same), ⌘] or ⌥⇧↓ skips to the next and ⌘[ or ⌥⇧↑ goes
back, ⌥↑ ⌥↓ step along the bar's sessions, the running ones too, and ⌃- and ⌃⇧- go back and forward
through the ones you looked at there. A strip over the header says how the last one went (a
message that didn't go after ⌘⏎ went on says why there, or on its session's pill once the notch has
closed), and has what's commonly done to the session as buttons that say their keys, as its
panel in Sessions has: Seen (⌘E), Done (⇧⌘E), Snooze (⌃⌘1 to ⌃⌘4), Close (⌃⌘W), Archive (⌃⌘A), Back
(⌃-), Next (⌘]) and × (esc). The PRs waiting on your review are in that same list, after the
sessions that need you and before the turns that ended, so the keys go from a session to a review
and on. On a review the strip has Approve (⌘⏎, with what's in the box as the review's text), Comment
(⌥⌘⏎, which needs text), Snooze (⌃⌘1 to ⌃⌘4), Review with (⌃⌘R starts one on the reviewer Settings names, else Auto's; the
button picks) and GitHub (⌃⌘G); Request changes is a button in the box, with no key, and needs text
too. Any of those settles GitHub's request to you, so the PR leaves what waits on you and the next
takes its place at once, in every window (the notch's count, the bar's pill, the mark), the review
going to GitHub behind it; one GitHub refuses comes back round with what you wrote, and the strip
says why. What you wrote stays with the PR until it goes, so
skipping and coming back loses nothing. ⌘E on a review skips it, as there's nothing to mark seen.
The pill of the one shown is marked. Marked seen, snoozed, closed or archived, a session leaves what
waits on you and the next takes its place; with none left the notch closes. Drag the session's
bottom corners or its bottom edge to size it: the notch stays centered, so a corner pulled out
widens it as much on the other side, it keeps the size across launches, and a double-click on a
corner gives it its usual size back. Escape, a click on its pill again or a click anywhere else puts
the session away and the app you were in has the keyboard back; if another app takes the keyboard
while the pointer is still on it, it stays until the pointer leaves. A count beside the camera shows
its group's first session, a session picked from a folded pill's menu or the recent ones shows there
too, a double-click on a pill still opens the session where it lives, and the pin in the session's
header keeps it on the desktop in a window of its own, where it was. A screen without a notch (an
external display) gets one drawn at the top center of its menu bar. The pair of buttons at the top
right of the window floats the bar or puts it in the notch, and picking the one that's on turns it
off, as do Window › Floating bar and Bar in the notch and the bar's own right-click menu.

### Pin to desktop

*A session's, worktree's, todo's or PR's right-click menu, in any view or on the canvas; or drag it
out of its window: a row out of the Sessions, Worktrees, PRs or Todo list, a result out of the quick
panel, a session's pill off the bar*

Puts its card, the one the Canvas view shows, on the desktop, floating over every window, on every
Space and over full-screen apps as the floating bar does. A dragged card follows the pointer from
where it leaves the window and stays where you let go; back over the window, the drag pins nothing.
A pinned row dragged to the desktop leaves its list's pins: it moves there. A worktree's card lists
its sessions that are running or waiting on you, a todo's its sessions, and a PR's the ones that
worked on it and are running or waiting on you; a click on a PR's card, or on a worktree's PR chip,
shows the PR in Codealong, and a double-click opens it on GitHub. Drag a card where you want it; it stays there across
launches. Click a session (its card, a session line, or a todo's card) to open its peek beside the
card, as the bar's pills do, with the keyboard in its box; click again to put it away. Double-click
to open what it shows where it lives (a todo without a session starts one in the quick panel).
Right-click it, or a session line in it, for that session's (Rename… renames it in its peek),
worktree's or todo's menu, where Remove from desktop takes the card off; the menu in Codealong also
offers that once it's pinned.

### Sounds

*Settings › Sounds, or `[sounds]` in config.toml*

A short sound plays when something starts to wait on you, so you notice while you're in another app
or away from the screen: a pop when a session needs you (it asks you something, waits on a call to
allow, or stops with an error), a lighter click when a session's turn ends (macOS's Pop and Tink),
and, once you pick one, a sound when a pull request asks for your review. Each can be one of macOS's sounds, one you put in
~/Library/Sounds, or None; the button beside it plays it, and Volume sets how loud they all are, as
a percent of the Mac's own. Codealong shows no Notification Center banners: the mark's beat and the
notch's count already show what came (*Queues, pills and the mark*), and the sound is for when you
aren't looking at them.

The sounds follow the bar's pills, whether the bar is up or not, so a snoozed session, the end of a
quieted one's turn, a helper that doesn't need you and a worktree kept out of sight make none. Each
Mac sounds for its own sessions: one on another of your Macs sounds there. To stay out of the way:

- nothing sounds for what already waits when Codealong opens, nor for the reviews already asked of
  you that GitHub's first read brings;
- a session that comes back to you within 15 seconds of going back to work makes none: you just
  sent it something or answered it, and are likely still looking at it;
- a few at once make one sound, the most urgent's (a session that needs you, then a review, then a
  turn that ended), and there's one at most every 10 seconds.

A sound config.toml names that isn't on this Mac would only be silent, so it shows as a notice, and
Settings won't save one.

## Opening things where they live

### Open where it lives

The session's own terminal tab (Ghostty, iTerm2 or Terminal, whichever it runs in; for a session in
tmux, the tab running `tmux attach` on its tmux session, with its pane brought up there, or, when
nothing is attached to that session, a new terminal that attaches to it), the Claude app
session, the Codex thread (a CLI thread's terminal, or `codex resume` in a new one), an OpenCode
session in its terminal (or resumed in a new one with `--session`), an Antigravity command line
conversation in its terminal (or resumed in a new one with `agy --conversation`), the Antigravity
app brought forward (it has no link to a conversation), a Cursor app chat in Cursor's Agents
window, a Cursor command line chat in its terminal (or resumed in a new one with `cursor-agent
--resume`), a background job attached in a new terminal,
or the folder in your editor. A Claude Code session running in a terminal or as a background job,
on the account the Claude app is signed into, also has Open in Claude app (⌘3) while it has Remote
Control (`/remote-control`, or `"remoteControlAtStartup": true` in its settings.json for every
session): the app shows it and sends it what you type there, and it goes on running where it is.
Without Remote Control the app can't take it while it runs, so the entry says so; one that isn't
running continues in the app (Continue in Claude app). A new terminal opens in the terminal app config.toml names, as a tab
in the window already showing that worktree, repo or project (`new_terminal` in config.toml;
Terminal can't open tabs for another app, so there it's a window). A worktree opens in your editor
(`editor` in config.toml, ⌘2) or, from Open in in its right-click menu and its Open button, any
other editor installed on the Mac: VS Code, Cursor, Windsurf, Zed, Sublime Text, Nova, BBEdit,
Xcode, the JetBrains IDEs and Android Studio, each with its app's icon. A session's right-click menu
and Open button, and cards pinned to the desktop, open its worktree's folder the same way, and ahead
of the editors, Open folder in your terminal: a new terminal in the session's own folder (the one it
was started in when that's inside its worktree, else the worktree's), placed as any new terminal is.
Picking another editor there makes it yours, as Codex's Open button remembers its last. A session
Codealong runs lives in its panel in the main window: from the bar, the notch, the waiting window or
a card, Show in Codealong brings that up, and in the panel itself the Open button leads with Open
folder in your editor. A session's tab that comes forward on another screen than the pointer's is outlined for a moment, and
`move_pointer` moves the pointer to it.

### Previews

A worktree's app opens in the browser (⌘5, its chip: a double-click on its row, a click in its
panel, under its folder, and in its sessions' panels; or its right-click menu). Its URL is set there
(Set preview URL…) or with `codealong preview set <url>`, which a repo's own dev scripts or
instructions can run, since only they know where a dev proxy serves each worktree's app
(`https://feature.app.test`). A server listening in the worktree's folder is linked to it: the one
on the port given with the URL, the port a localhost URL names, or else the one whose front page is
a page, not an API or a debugger. The chip, the same everywhere, is green while that server runs and
outlined when it doesn't, with the full sentence in its tooltip; the panel says which process serves
it, for how long, its page's title, and the session whose command started it. A worktree with no URL
set that serves a page shows it at `localhost`. A preview can open in a Chrome profile, with that
profile's sign-in and cookies: a project's `chrome_profile` (Settings › Projects) covers its
worktrees, and a worktree can pick its own in its Preview URL sheet or with `codealong preview set
--chrome-profile <name>`. Profiles are named as Chrome's profile picker shows them, by the account
signed in to them, or by folder, and are read from Chrome's `Local State`. A link into a worktree's
app, in a conversation or a PR's description, opens in that profile too: one at its URL's host and
port, or at the port of the server that serves it; any other link opens in the default browser.
Servers are found from the kernel's list of your processes' listening ports every 10 seconds (a few
milliseconds for every worktree at once), each new one is asked for its front page once, and the
kernel says the moment one exits, so nothing is polled over HTTP. Only worktrees in projects are
looked at, and only on this device.

### Chrome tabs

A Claude Code session that drove Chrome through Claude in Chrome has Show in Chrome in its
right-click menu and its panel's Open menu, naming the Chrome profile when Chrome has more than one.
It selects the session's own tab in whichever window has it, which shows its tab group, and brings
that window forward. Once its tabs are gone, the item reads Reopen <site> in Chrome and opens the
session's last page again in that profile. The tabs come from the session's transcript (each of the
extension's results lists the session's tab group); the profile from the extension's storage in each
profile, which records its tab groups, or from each profile's History for a group it no longer
remembers.

### On Linux

Codealong runs on Linux too, under X11, where a terminal emulator takes no scripts. So there
`terminal = "terminal"` means the desktop's terminal: each new terminal is a tmux session, which
Codealong types into and presses keys in, shown in a window of the first emulator found
(`$TERMINAL` when it names one, else Ghostty, kitty, WezTerm, Alacritty, foot, Konsole, GNOME
Terminal, Ptyxis, Xfce Terminal or xterm). A Background start opens no window, and Show in later
opens one attached to it. Show in raises a session's window with `xdotool`. A session started by
hand in an emulator's own tab, outside tmux, can be opened but not typed into or found. Editors
are found by their commands on your PATH (`code`, `cursor`, `zed`, `subl`, `idea` and the rest),
and `editor` in config.toml can name any other command. Links open with `xdg-open`, a Chrome
profile's with `google-chrome --profile-directory`, and Show in Chrome opens the session's last
page again, since Chrome takes no scripts there. The windows that float stay over other windows on
every workspace, without the notch, and need a compositor for their clear parts. A sound plays
through `paplay` from the freedesktop sound theme. What the Mac has and Linux doesn't is listed in
the [development notes](development.md#linux).

The windows' keys are the Mac's, pressed as Linux has them and said in words: ⌘ is Ctrl, ⌥ and ⌃
are both Alt, and ⇧ is Shift, so ⌘E is Ctrl+E and ⌃⌘W Ctrl+Alt+W. A few go otherwise where that
would make two the same or take a key the desktop keeps: a project is Alt and its digit (⌥⌘1 on
the Mac, where ⌃⌘1 snoozes), the views are Ctrl+Tab and Ctrl+Shift+Tab, a terminal Ctrl+`,
send now Ctrl+Shift+Enter, and moving a pinned row or a todo Ctrl+Shift+PageUp and PageDown (Ctrl+Alt
and an arrow switch workspaces). The page never sees Super, so no window key uses it. The shortcuts that
work from any app are written in config.toml as the desktop's keys, where `Cmd` is Super, and
Settings records them with Ctrl and Alt; they're Ctrl+Alt and a key by default.

## What Codealong works out

### Session status from Jev

*Optional; Settings › Jev, `[jev] sessions`*

Once per turn, Jev reads the end of the last reply, with the latest prompt it answers and the
first prompt the session was started on, and the command a session runs or waits on. A reply that
asks you something it waits on (a question, a choice, a go-ahead, or a sign-in only you can do)
turns *Your turn* into *Needs you*, with the question; one that offers more work ("say if you
want…") or leaves you next steps to take yourself stays *Your turn*. A busy session quiet on a
long build stays *Working* instead of *stuck*; a command left running in the background says
whether it polls for something, runs a job, is a server or watcher left running (no longer
*Working*), or waits on you (a sign-in, a password), which still takes what you send it, since its
turn is over. Rows show how far the work got (Landed, Pushed,
Committed, Not committed, Blocked) and how risky a command waiting for approval looks, judged from
the whole call where its harness shows it (the arguments, not just the command's name); a worktree
whose sessions all read as done says *Looks finished*. A reply whose first line opens with the
status line CLAUDE.md asks agents for (`Landed:`, `Pushed:`, `Committed:`, `Not committed:`) says
its landing itself, exactly, and Jev isn't asked it. A turn its harness ended (the connection lost,
a usage limit, cut off) isn't read: its reply is an error. Answers are kept in insights.json, with
how sure Jev was, so a turn is asked about once.

#### Tags from an agent

An agent that reads a session can say the same things itself, with `[jev] sessions` on or off: an
orchestrator going over the sessions it started (`codealong sessions`, then `codealong session
transcript` for each) tags each one with `codealong session tag` (asks you, how far the work got,
finished, how risky the waiting call is, what a background command does, and why in a note). A tag
shows exactly as Jev's read does, in the same chips and status, and stands over Jev's for the
same moment where the agent said; a field it left out stays Jev's. The chip's tooltip says who
tagged it and the note. A tag is for the reply the agent read: one written after you'd replied
since, or while the session takes a turn, is turned down, and it goes once anything is sent to the
session or a turn of its starts; tagging again replaces it. Tags are kept in tags.json; a session
on another device keeps its tag on its Mac.

### Commands left running

A Claude session whose turn left a command running in the background (registry status `shell`) is
*Working* while that command does something. The engine looks at the command's processes once a
minute; when none has started or used CPU for 15 minutes (a dev server, a sign-in waiting on a
browser, `tail -f`), the turn is over and the row says *Left running*.

### Where a session came from

A fork of another session, or one another session's agent started. Codealong reads what the
harnesses record (Claude Code's `/fork`, `/background` and forked background jobs; Codex's forks and
the threads its app makes for an agent; OpenCode's forks) and notes what they don't: a session
started by an agent's `codealong start`, and a fork into a terminal made from Codealong or a move to
another account. A session's panel names the session it was forked from, moved from or started by
over its title, and shows the latest of what came of it under the title, with its status and a "+N
more" for the rest: the sessions forked from it or that its agent started (its helpers among them,
*Its helper*), its headless runs on one line with how many, and its side conversations, each by what
it was first asked. A click shows a session and a right-click gives its menu, so Pin to desktop there
puts its card beside the one you're on; a click on the runs shows the latest, listed under the
session in Sessions, and one on a side conversation shows it above the box. The bar's peek and the waiting window have the same header, and
its card's popover on the canvas lists them too. On the canvas, hovering or selecting a session
draws a line from the session it came from and to the ones that came of it, and a session's card on
the desktop does the same to the cards of those sessions pinned on the same screen.

A session can message another with `codealong session send` even once that one has stopped:
Claude Code's background service ends a job left idle at its prompt for an hour, so the session
that asked for some work often isn't running by the time it's done, and one coordinating others
finds them stopped when it has more for them. Codealong resumes the stopped one out of sight (Claude
Code's background service, else in Codealong or a terminal behind your windows) on the account's
own model, with the message. The message starts with a line saying the session had stopped and was
resumed for another session's message, then with the sending agent's, so it's read as neither yours
nor leave to do what you haven't allowed; the sender is told it wasn't running and was resumed.
Never one you archived or marked done, one that goes on in the Claude app, or one that skips
permission prompts when the sender doesn't, which would hold the message for you if it were running
(plan mode, and a mode its transcript doesn't say, count as skipping them); the sender is told why.
Nor again within two minutes of resuming it for another message, while the board may still show it
stopped: the sender is told to send it again once it runs. In the conversation, the message shows
as the sending session's.

`[sessions] resume_for_any_session = false` (Settings › Automation) keeps it to sessions linked by
`codealong start`: the session that started another, and the ones it started, when the start said
which session it made (as for helpers, below). `resume_for_sessions_it_started = false` as well
turns it off, and nothing is sent to a session that isn't running, as for any other agent's
message.

### Helpers

A helper is a session that works for an agent rather than for you: one an agent started with
`codealong start --helper` (a review of its diff, a check, a second opinion on another account or
model), and an automatic review's session, which helps its PR, and for a PR one of your sessions
opened, that session too, as the review goes back to it. So is a session an agent ran in its
session's own scratch folder (Claude Code's scratchpad, or a background job's folder), a test chat
in tmux, say: it helps that session, which isn't told of it. Nobody is expected to talk to it,
so a turn it ends is idle rather than Your turn, and while it doesn't need you it's out of its
worktree's status, the floating bar, the notch, the waiting window, the quick panel's lists and the
mark. Started without `--in`, it runs out of sight, as automatic reviews do.

In Sessions a helper is listed under the session it works for while it works or waits on you: on
one line, set in, joined to the session by a thread from the session's status, and naming its
worktree only when it isn't the session's. The session's row has a chip with each of its
helpers' status glyphs, the most urgent first (three, then +N). Resting the pointer on the chip
brings up a card with every one of them: what it's called (*Review of #204* for an automatic
review), where it stands since when (*Working for 3m*, *Done 12m ago*), what it's doing, asking you
or last said, and who runs it, on which model, in which worktree when it isn't the session's. A
click on one shows it, listed under the session (a finished one too), with its conversation in the
panel; its arrow opens it where it runs, and a right-click has its menu. A double-click on the
chip, or the card's *List them all under it*, lists all its helpers under it, finished ones too,
until another, or until the row at the end of what's listed under it (*Hide 3 finished helpers*)
takes them away again; Codealong remembers that across launches. Its panel lists them all under its title, as *Its helper*. The helper's own panel
says *Helper of* that session over its title, and an automatic review's says *Review of* its PR.
A helper whose session isn't listed (filtered out, or a review of a PR that waits on your review,
which has no session of yours) is listed on its own only while it waits on you, with a *Helper* or
*Review of #N* chip; a search finds every one.

A helper that needs you (a call to allow, a question, an error) waits on you as any session does,
so its agent isn't left waiting on it unseen: its worktree goes to Needs you, it's on the bar and
in the queues, and in Sessions it takes the session it works for into Needs you with it, its row
under that session's. The session's chip turns amber while one waits on you.

The agent that started it reads its reply with `codealong session wait <key>`, which waits for
its turn to end, or comes back as soon as it waits on you so the agent can say so (`--until-done`
waits through that, for after it has), and ends it with `codealong session done <key>` once it
has what it needs. Started with `--report` as well, its session is sent a message from Codealong
when the helper's turn ends or it starts waiting on you, so the agent can end its turn meanwhile:
the message names the helper and the command that prints its reply, not the reply itself, so
nothing the helper wrote is typed in as if you'd written it. It's sent while the session runs on
this Mac and isn't waiting on you to answer a prompt, several helpers' news in one message, and a
send that doesn't go is tried again a few times, then said in a notice.

A start in a terminal whose harness names its own sessions (Codex, OpenCode, Antigravity) has no
session until its terminal UI makes one, and the UI can first stop on a screen of its own (Codex's
question about its background server) that Codealong can't read or answer, in a terminal that may
be behind your windows. Codex's terminal UI as Codealong opens it (a start, a resume, a fork) never
offers an update; Codex you run yourself still does. When one started with a prompt, by you or by an
agent, has made no session three minutes on, a notice says where (*Codex hasn't started in …*)
until its session shows, and a helper started with `--report` is told of to its session once, so
its agent can tell you.

A helper's message to the session that started it (`codealong session send`) isn't held for
you to allow when that session is a Claude Code one that skips permission prompts: it's work the
session asked for, and would read anyway. That holds only when the start said which session it
made, as one in Codealong, in Claude Code's background service or in Claude Code in a terminal
does; a helper found afterwards as the next session in its worktree (Codex, OpenCode or
Antigravity started in a terminal) could be another session started about then, so its message
is held as any agent's is. Such a start takes the next session of its account in its worktree for
half an hour, and after that, for up to a day, only one that opens with the prompt it was started
with and whose process began as the start did (or, its process gone, that began then itself, as
one found only once the app is opened again): a terminal UI can stop on a screen of its own
before it makes its session (Codex's question about its background server) until you answer it. A session whose process
began before a start was asked for is never that start's. So is an automatic review's, whose reply reaches the PR's opener only
as `[reviews.own] send` says, and that of a helper you made your own; a start another Mac asks
for makes no helper. A message from any other agent is held there, as before, unless the agent's
own session skips them too.

A message from another agent, or one Codealong sends by itself, reaches a Codex session that's
taking a turn in its terminal or in the Codex app in that turn, which reads it at its next step, as
a Claude Code session takes one between its steps; in the app that holds even when it's set to
queue your own follow-ups. One to an OpenCode session, or to a session Codealong runs, waits for
its turn to end.

Mark as done on a session marks its helpers done too, as their work was for it: each ends as far
as Codealong can end it and is archived, and one still at work is left to finish, unless you
chose to stop the session's work too. *Make it my own* in a helper's menu makes it a session of
yours for good: listed, counted and waiting on you as any of yours, and no longer told of to the
session that started it. A helper whose session is gone from the board is a session of your own
again. Macs from before helpers show another Mac's helpers as sessions of their own.

### Headless runs

A script or an agent can run an agent on a prompt with nobody at it, and read what comes back
itself: `claude -p`, or `codex exec`. An eval that runs its judge that way can start hundreds. Each
is a session in Codealong all the same, marked headless (`headless` in `codealong sessions`
JSON), but a turn one ends is idle rather than Your turn, an error and all, since what ran it
reads how it ended. One waiting to have a call allowed still needs you.

While it doesn't need you, a headless run isn't an agent of yours at work: it's out of its
worktree's status, the status filters and their counts, your devices' lines, the floating bar and
the mark, as a helper is. Codealong looks for the session that ran it while it runs: the nearest
of the run's parent processes that's a session's own, as a command an agent runs goes on under
that agent (an eval's `claude -p` under python, under the shell the agent's tool opened). Where
none is, as for a script an agent set going on its own to carry on after its command returned,
it's the session the run's environment names: a Claude Code session's commands are given its ID
(`CLAUDE_CODE_SESSION_ID`), and whatever they start keeps it. That
session is where the run came from (its header says *Started by*), and Sessions counts its runs on
its row rather than listing them, a row a call: a chip says how many, with the working glyph while
any is at work. Rest the pointer on it for the helpers' card for them: each run (the first 20),
the ones at work first, with where it stands since when, what it's doing or last said and what it
runs on; a click on one lists it under the session with its conversation in the panel, its arrow
opens it where it runs, and a right-click has its menu. Double-click the chip, or use the card's
button, to list them all under the session, and again (or the row at the end of the list, *Hide 4
headless runs*) to put them away; that's kept apart from listing its helpers. A run started with `--no-session-persistence` keeps no conversation to read,
and leaves the board as it ends. A run no session
on the board ran (a script in a terminal of yours, or one that ended before Codealong saw it
running) is listed as a session of its own, with a *Headless* chip; so, for now, is every `codex
exec` run, whose process Codealong can't yet tell from its thread. Once a run that keeps its
conversation has ended, Codealong notes its session with the other sessions' origins
(`origins.json`), so it stays under that session after Codealong starts again; nothing on disk
would say so otherwise. A search finds every run, opening one from the quick
panel lists its session's runs, and a session's panel names them on one line under its title,
with how many.

`codealong sessions` leaves them out unless you ask for them with `--headless`, and `codealong
changes` counts them by the folder they ran in instead of listing them, and doesn't take several
in one folder, or one beside an agent, for a shared worktree. Claude Code marks a run headless
itself (it names it `sdk-cli`, and leaves it out of its own `/resume` list); a session started
through the Agent SDK isn't one, since that's often a front end someone types into. OpenCode,
Antigravity and Cursor don't say, so their print runs are sessions like any other.

### Usage

**Usage** for every account in a bar across the top: 5-hour and weekly limits, per-model limits such
as Fable, and time to reset. When they don't all fit, each account shows the limit it's closest to
running out of and the rest on hover, and the bar scrolls sideways. Antigravity's are its weekly
limits for each group of models (Gemini; Claude and GPT), read through its command line, `agy`,
every couple of minutes; with only the app installed it has none to show. Cursor's are its monthly
limits, asked of Cursor with the app's sign-in every couple of minutes: on a plan that spills over
(Pro+, say), one for Cursor's own models (Auto, Composer and Cursor Grok) and one for the others,
as the usage line under Cursor's chat box has them; on Express only the first; on any other plan
the total Cursor's Settings shows; and on-demand spend once you've set a limit on it. OpenCode (bring your own provider) has none, and
its chip says so with a crossed-out gauge after the name. Add accounts in Settings › Accounts, and
turn off one you'd rather not see here.

The chips are in the order config.toml lists the accounts. Drag one sideways to put it in another
place: the others make way as it goes, Escape puts it back, and letting go moves its
`[[accounts]]` table in config.toml, as written and with the comment right above it. Settings ›
Accounts lists them in that order too, and of two accounts equally good for Auto (without a
routing group), it starts the one listed first.

#### Training on what you send

Each account's provider may be allowed to train its models on what the account sends. Codealong
asks every provider where it keeps that setting, at launch, every hour and when you click the
bar's refresh, and an account whose provider may train on it, or where nothing's been chosen yet,
has its chip in amber with a crossed-out shield. Its card, on hover, says which setting it is and
when it was read, and stays up while the pointer is on it: **Turn off training** turns it off from
Codealong and reads it back to check (a toast says how it went), and **Open settings** opens the
provider's page for it. A click on the chip still opens the Usage window. Settings › Accounts says
where every account stands (*Not trained on*, *Training on*) and offers the same.

- Claude (Pro and Max): *Help improve Claude* in claude.ai's privacy settings, one setting for the
  account in claude.ai, the Claude app and Claude Code. On, Anthropic keeps chats and coding
  sessions for five years rather than 30 days. Team and Enterprise plans are under Anthropic's
  commercial terms, which don't train on what you send.
- Codex: *Improve the model for everyone* in ChatGPT's data controls, and Codex's own training
  setting beside it. ChatGPT Business, Enterprise and Edu workspaces aren't trained on. An API
  key's calls aren't either unless its organization chose to share them, which only OpenAI's
  dashboard says, so Codealong can't tell for one.
- Cursor: *Privacy Mode*; *Share Data* lets Cursor keep and train on your prompts, edits and code.
  When your team sets it for you, the flag says so, and it can't be changed from here.
- Antigravity: *Enable Telemetry* (its Settings › Account, and `/config` in `agy`), which lets
  Google collect your interactions to improve Antigravity and the models behind it. Its sign-in is
  refreshed as `agy` runs, so while that hasn't run for an hour it may not be read.
- OpenCode sends your work to whichever providers it's signed in to, and each one's own settings
  say whether it trains on it; Codealong doesn't read them.

#### What sessions used and cost

*View › Usage… (⇧⌘U), the bar's Usage button, or a click on an account's chip*

The Usage window says what your sessions used over the last 7, 30 or 90 days or the year, for every
account or one: what the work would have cost at API prices, what you paid, how many dollars of
work each dollar paid bought (leverage), and the tokens, with how much of what went in was read
from the cache (most of it, and at a tenth of the price or less, which is why a token count alone
says little). Under them, the running totals at API prices and paid over those days, and each
day's cost by account; then a table by account, model, project, session or day. A session's row
shows it on the board, an account's shows that account alone. A session's details (the ⓘ in its
panel) say what it cost and used too, its subagents' calls included.

What was paid is what Codealong knows of: each account's plan, `monthly` in config.toml (Settings ›
Accounts, *It pays*), counted a day's share of its month at a time; Claude's extra usage, as its
usage endpoint reports this month's (read with the limits, every 2 minutes); what OpenCode
recorded a call cost, which is what your provider charged; and an account that pays by the token
(`per_token = true`, an API key) at API prices. An account with none of those adds nothing to
Paid.

The numbers come from the records each harness keeps of every model call: Claude Code's
transcripts and its subagents' logs, Codex's rollouts, OpenCode's database. Codealong reads them
into a ledger of its own (`ledger.sqlite`), each file once and then what's new, every minute, so the
history outlives the transcripts Claude Code deletes after about 30 days; the first read after
installing goes back as far as those records do. A call that's written more than once (a reply
as it streams, a fork's or a moved session's copy) counts once, for the session that made it.
Cursor and Antigravity keep no record Codealong can read, and are named as such.

API prices are the list prices of Anthropic and OpenAI per token, by kind: input, cache reads,
cache writes kept five minutes or an hour, output with its thinking, web searches, fast mode and
long prompts, as their pricing has them. They come with Codealong; a model it has no price for is
counted in tokens and said to be unpriced, never as free, until a price for it goes in config.toml:

```toml
[prices."some-new-model"]   # dollars per million tokens; a cache price left out is the input's
input = 3
output = 15
cache_read = 0.3
cache_write = 3.75
cache_write_1h = 6
```

`codealong tokens [--days N] [--account <name>] [--by account|model|folder|session|day]` prints
the same from the command line.

Codealong's own questions to TypeSafe's Jev aren't a session's work, so they're in Settings › Jev,
under Usage: what they cost today or over the last 7 or 30 days, how many there were and their
tokens, and the same for each feature that asked (the Test button's too), the one that sent the
most first. TypeSafe charges for what's sent, $0.042 a million tokens, and Jev's answers are free.
Each answer says what its request used, and the ledger keeps it; a request TypeSafe refused isn't
counted. Counting began with the version of Codealong that added it, and Usage says since when for
a period that reaches back past that.

## Tidying up

### Close idle sessions

*File › Close idle sessions…*

Lists the sessions that sat idle for a day or more (12 hours to a week, you choose) and asked you
nothing in their last reply, by Jev's read where it has one, else by it not ending in a question. It
says why for each and what closing does to it, and closes the ones you leave ticked: a Ghostty,
iTerm2 or Terminal tab closes, which ends the agent's process, and a Claude Code background job is
stopped. Nothing closes by itself, and the conversation stays and resumes. With **Also archive
them** (remembered), each is marked seen and archived too, as Mark as done does, and the sessions
Codealong can't close (in the Claude or Codex app, run by Codealong, in tmux) are listed to be
archived only, staying open where they are. `codealong sessions close-idle --dry-run [--archive]`
lists the same from the command line.

### Clean up worktrees

*A worktree's right-click menu › Clean up worktree…*

Removes a worktree whose work is done, only when nothing in it would be lost: no file uncommitted or
untracked (whatever `status.showUntrackedFiles` says, and tracked files git is told not to look at
counted by their contents), no commit that isn't on some remote (as `git rev-list HEAD --not
--remotes` counts them, Claude Code's own rule), no session running in it, no lock a running
session holds on it, and no other worktree or repository inside it, every ignored folder looked
through for one (a worktree with its dependencies installed takes a few seconds). A git command
that fails keeps it. A main checkout, a plain folder and a reusable worktree are
never removed. Files git ignores, such as installed dependencies and env files, go with it. Where it can't go, the
item is dimmed and says why: "3 uncommitted files", "2 commits on no remote", "a session is running
in it", "the main checkout", "marked reusable". Worktrees picked together offer Clean up N
worktrees… for the ones that can go.

It asks first, since it can't be undone, and lists what would run for each: when the repo's
`.claude/settings.json` (or `settings.local.json`) declares a `WorktreeRemove` hook and a Claude Code
account trusts the repo, the hook runs as Claude Code would run it, in the main checkout with the
event on its stdin, so a repo that sets up more than a folder for each worktree (ports, routes,
databases) takes all of it down; otherwise `git worktree remove` (never forced), then `git branch -d`
for its branch when no other worktree has it, which keeps a branch whose work isn't merged. A lock a
session left behind as its tab closed comes off first. Then, for a Claude Code background job that
held the worktree, `claude rm <job>` drops the job's record; its conversation stays. Everything is
checked again as it goes, one removal at a time and once more just before it goes, and a worktree
that changed since (a new commit, a file, its lock) stays and says why. A hook that removes the
folder but fails at the rest of its teardown is said to have failed.
The toast says what was done. On another Mac the worktree is removed there, with remote control on.
Worktrees outside your projects (Elsewhere) aren't read by git on the board, so their menu doesn't
offer it; `codealong worktree remove <path> [--dry-run]` does, from the command line, and
`codealong worktrees` says of each whether it can go.

## Your other devices

*Settings › Devices; off until you turn it on*

Codealong on each of your devices shows the others' worktrees and sessions beside its own.
Everything from another device wears that device's badge, a colored square and its name (pick each
device's color in Settings › Devices); other devices' worktrees go into this device's projects by
their repos, whatever those devices call them, even where a checkout still has a renamed repo's old
name; work on a device that's away stays, faded, as it was when last seen; the sidebar's Devices
narrow the views to the ones you pick. A session on another device shows its whole conversation in
its panel; you can mark it seen or snooze it from here, and when that device lets its linked devices
act on its sessions (remote control, off by default) send it a message, answer an approval or mark
it done. The images you paste into a message or a start for another device go there with it, and
the images in another device's conversations come from it as they show; any other file you attach
stays on the Mac you attached it on, so a message with one for another device isn't sent. Devices
find each other on your tailnet when Tailscale runs, and reach linked devices
anywhere through iroh; they talk directly, encrypted end to end, with no server of Codealong's in
between. You link a device once: on your tailnet, both show the same six digits and you say yes on
the other one; elsewhere, with a link code made on one device and entered on the other. A device
is reached while Codealong runs there, its windows open or closed (closing the main window only
hides it); once it quits, or the Mac sleeps, it shows as it was when last seen. Open at login, in
Settings › General, has it running again after a restart without your opening it. Each
session goes out with its place among its Mac's pins, so the iPhone app lists a Mac's pinned
sessions first, in that Mac's order, and pins or unpins one there, remote control or not.

## For agents

The `codealong` command and a skill that tells an agent when to use it. `codealong whoami` says
which session runs it (found by its processes, for a session Codealong runs by the ID its harness
gives the commands it runs, and for the Codex app and OpenCode by the session's own record of the
command), the other agents running in its worktree, and its open todos (the
worktree's too, when no other agent runs there); a worktree's agents are the sessions running in it,
not every session that ever worked there, nor an app's unrelated chats in its chats folder or a
headless run; `codealong todo
add|start|done|update|move|wait|ready|link` work the todo list, dependencies, waits and order
included (`todo list --ready` is what can start now, `--waiting` what's under Waiting), and starting
or finishing a todo links the session that ran the command, unless `--no-link` says it closes a
todo for you that other sessions worked on; `codealong sessions [find <words>]`,
`session show`, `worktrees` and `accounts --usage` answer what else is running, what it did, where
it lives, and which account has room; `codealong session open` brings the main window up on a
session's conversation; `codealong session transcript` reads a session's conversation whatever its
harness, `codealong sessions --waiting` lists what waits on you, and `codealong session suggest`
puts replies above a session's box for you to send with a click; `codealong session
stop|rename|fork` do what a session's menu does; `codealong terminal run` asks you to run a command
in your terminal and hands back what it printed (see *Commands in your terminal*), and `terminal
read|wait|stop|list` read, wait on, interrupt and list those runs; and
`session pending` and `session answer … deny` what its panel does, except letting a call through or
answering its questions, which is its user's to do; `codealong session done` is Mark as done for
what's the agent's own: its own session (`me`), which ends once its turn does and stays in Your turn
for you to read, and a session it started, never one working or waiting on you, while the main
window says which session did it, with an Undo; `codealong start --todo <id>` starts a session the
way New session does (`--todo` again for several todos in one session), and `codealong start
--helper --prompt …` a helper of the session running it (see *Helpers*), whose reply `codealong
session wait <key>` waits for, or which `--report` has Codealong tell the session of; `codealong config
show|check` prints the settings and checks an edited config.toml. With the app running, answers come
from it over a socket and todo changes go through its engine; with it closed, the command reads the
files (sessions then show without git or PRs). Output is JSON when piped, tables in a terminal.
Settings › Agents installs the command (a symlink on PATH), the Codealong plugin in every Claude
Code account (the skill, the command for its Bash tool, and the hooks that let you answer its
permission prompts and questions from Codealong), and the skill in every other account's skills
folder (`~/.codex/skills`, `~/.config/opencode/skills`, `~/.gemini/config/skills`,
`~/.cursor/skills`), and keeps them
current; `codealong install` does the same from a terminal. Settings › Agents is where each
account's plugin or skill shows. `codealong` alone opens the app as the Dock does: macOS opens a
new copy, which takes over from the one running, with the environment it gives an app rather than
the terminal's, so nothing the app starts carries an agent's session with it. See *For agents* below.

### An orchestrator

A session can keep watch over all your others for you, on a loop. Start one with `codealong start
--prompt "/codealong:orchestrate"` on a Claude Code account, or `/codealong:orchestrate` in a
session's box; on another harness, ask it to run as your orchestrator with the codealong skill. It
asks first how often to look (every 15 minutes) and which optional parts you want, keeps your
answers, its cursor and a running narrative of the work in `orchestrator/` in Codealong's folder,
and schedules itself with its harness's scheduler. Each tick it reads what changed, tags what waits
on you and suggests replies (*Suggested replies*), and points out agents getting in each other's
way: two sessions in one worktree, two on one task, PRs touching the same code, a production change
planned while a deploy runs. It never messages a session for you. Where Codealong itself gets in its
way (a command that misleads, output it has to filter, a session shown in the wrong state), it
notes it in `orchestrator/codealong-ideas.md` and suggests the fix once; with your go-ahead, each
time or once for all of them, it starts a session for it where Improve Codealong would, after
checking that no todo or session has it already. If you say so, it also ends what's
left running of old sessions (`codealong hygiene --kill`), turns real action items in your email
into a todo and a session that may only investigate and draft (with a Gmail connector), watches
deploys where a repo has a watch of its own, and once a day looks at what's new in Codealong and
suggests how its loop or your work could use it. The skill is `references/orchestrate.md` in the
codealong skill.

Its two commands are for any agent. `codealong changes` says what changed since the cursor an
earlier run printed: sessions that started, began or ended a turn, wait on you or stopped, each with
what began its latest turn and its reply (your latest prompt, or a message another session or
Codealong sent after it, marked as theirs); PRs opened, out of draft, approved, sent back, merged or closed; todos
added, started, done or set waiting; worktrees a second agent started working in (not an app's
chats, listed together only for having no folder of yours); and how many headless runs (*Headless
runs*) changed in each folder, rather than a line each. With them comes every session that waits on
you now, and whether each has suggestions or a tag. It tells by difference from what that run saw,
kept for a week in `changes.json`, so a busy app or GitHub not answering never makes a session or a
PR look new, nor a todo look removed while the app starts: what it couldn't read it says, and tells
once it can. `--cursor-file` keeps the cursor in a file, so a tick is one command. `codealong hygiene` lists
what's left running of old sessions, with the Mac's load, memory and swap: the agent process of a
session quiet for a day (not one working or waiting on you, unless it's archived), older copies of
a background job's attach client, a harness's background processes whose parent has gone, and dev
servers and watchers in a linked worktree no session has been active in for a day, never in a main
checkout. `--kill` ends them with their child processes, each checked again first and logged, a
background job stopped the way Close stops it; it judges nothing when the app doesn't answer.

## Files Codealong keeps

Everything lives in `~/Library/Application Support/Codealong/`, beside `config.toml`; on Linux
in `~/.config/codealong/` (`$XDG_CONFIG_HOME`), with the logs in `~/.local/state/codealong/` and
caches in `~/.cache/codealong/`.

What you've seen (for Your turn), what you've pinned, snoozed and archived in Codealong are kept in
`state.json` next to it; whether the bar is on, floating or in the notch, and where you left it
floating, in `bar.json`; what's pinned to the desktop, and where each card was left, in
`desktop.json`; the sessions kept there from the peek, and their windows' frames, in `kept.json`;
each project's canvas in `canvas.json`; which session started or was forked into which (and which
started ones are its helpers), where the harness doesn't say, in `origins.json`; whether your
latest quick-panel starts in each repo went into a new worktree, in `starts.json`; sessions linked to PRs or unlinked from them by hand, the
PRs nudged or not and the sessions picked as their primary, in `pr-links.json`; worktrees marked
reusable or not in `worktrees.json`; where each PR's automatic reviews stand in `reviews.json`;
replies suggested for sessions, until the sessions move on, in `suggestions.json`; each session's
emblem, the color Codealong gave it and the icon Jev picked, in `emblems.json`; each command you
allowed an agent to run in your terminal, with what it printed, for a week in `terminal-runs/`;
what `codealong changes` saw at each of its last dozen looks, for a week, in `changes.json`; an
orchestrator's answers, cursor and narrative in `orchestrator/`; what every
model call used, and Claude's extra usage as it was read, in `ledger.sqlite` (the Usage window's).
Todos and labels are in `todos.json`, which is watched, so an agent or an editor can add to it.
Attachments are kept by their content: images in `images/`, other files in `files/<hash>/<name>`;
ones no todo uses are cleared after 30 days. The devices this one is linked with are in
`devices.json`, and what each last said in `device-feeds.json`; this device's key is in the login
keychain (on Linux, the desktop's Secret Service).

## What's next

The menu bar icon and popover, and notifications (the design system's *Build plan*).
