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

**Five views** of the current project, each in a window of its own if you like (⌘⇧N). The project
switcher (⌘P) shows each project's key: ⌥⌘1 to ⌥⌘9 switch projects, ⌥⌘0 shows them all. Add
project… at its foot opens Settings › Projects with a new project's form, and Edit projects… the
list. ⌃1 to ⌃5 go
straight to Sessions, Worktrees, PRs, Todo and Canvas, from a text field too, and a view in the
sidebar shows its keys while you point at it (the folded rail says them); ⌃Tab and ⌃⇧Tab go through
the views (`v` too, outside Sessions), ⌥↑ ⌥↓ move along a list as the arrows do, and ⌃⌘↑ ⌃⌘↓ reorder
what's pinned. ⌃- goes back to the session you were looking at before and ⌃⇧- forward again, as in
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
shows the PR in Codealong (its panel in PRs, or its card on the canvas when the canvas has one), and
a double-click opens it on GitHub; a PR Codealong doesn't list opens on GitHub on a click too. A
click waits a quarter of a second for a second one, so a double-click doesn't show the PR first.
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
(drag or ⌃⌘↑ ⌃⌘↓), each with a pin under its age. Drag any session up among them to pin it there,
and a pinned one down to the rest to unpin it. The bar over the list has two groups of icon buttons,
each named in its tooltip. Sort by orders the rest: Status puts Needs you, Your turn and Working
first, as the floating bar orders them, then Snoozed, the soonest back first, then the rest under
the day they were last active (Today, Yesterday, Last 7 days, Older), the one you last sent a prompt
first. Started puts the newest session first, under the day it started, and each row's age then
says when it started. A row leads with the session's title, then what it's doing or said last; one
that needs you has a label beside its title for what it waits on (Asks you, Needs approval,
Message, Stuck, Error). Its second line says where and why: its worktree, its todo and Jev's read,
then its account, with the worktree's PR and diff at the right. Rows, after Sort by, puts each
session on one line instead (its title, then its worktree and PR at the line's end); there
its status says its account when you point at it. A snoozed session's row says how long until it's
back, and a quiet one's says Quiet. Here letters alone do nothing, so a word typed on the list by
mistake doesn't pin, archive or rename: the keys are the chords, and they work from the panel's box
too. Sessions are pinned here, with ⌃⌘P or the right-click menu, and their pins show only here.

#### Archive, close and mark as done

Archive a session with ⌃⌘A (`a` elsewhere) or its right-click menu; Codex threads are archived in
Codex itself. Close terminal, in the right-click menu of a session running in a Ghostty, iTerm2 or
Terminal tab, closes that tab whatever the session is doing, and End background job stops a Claude
Code background job; either way its conversation is kept and resumes. Mark as done (⇧⌘E, or Done on
the strip) is for a session you're finished with: it closes the session's terminal or ends its
background job, as Close does, and marks it seen and archives it, so it leaves every list but
Archived. It closes nothing that would take more than the session with it: a terminal that shows
other sessions too stays open, and a session in the Claude or Codex app is only archived, since the
app has no way for another program to close it. A session still working, or one that left a command
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
its todos, among the pinned sessions, on its card on the desktop and in its kept window. Claude Code
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
lifts, from what Claude Code wrote), and its panel offers to move it to the account with room: one
of its harness's other accounts, chosen as Auto chooses, that isn't near a limit by its meters and
whose own sessions weren't refused since they last got through (a spend limit shows on no meter).
Moved, it's told to carry on where it stopped. `[limits] when_hit = "move"` (Settings › Automation)
moves it there by itself, as soon as it stops; a session moved there that stops again is offered a
move rather than moved back.

#### Network errors

A Claude Code session whose turn ended because it couldn't reach its API says so (Network error):
with the lid closed the Mac wakes for a few seconds every minute or two without DNS, and Claude
Code's retries run out there. Codealong tells it to carry on once the API can be reached again, once
for each time, and leaves it for you (with a notice) when it loses the connection again straight
after a few times running; `[sessions] continue_after_network_error = false` (Settings › Automation)
turns that off.

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
not the session (its placeholder says Ask on the side): type a follow-up and press ⏎. Close ends it
and gives the box back to the session, and a copy left idle for 15 minutes ends by itself. Codealong
keeps what was asked and answered, with what the copy did to answer (`sides.json`, the latest 300):
`/side` alone, or Earlier beside an open one, lists a session's side conversations from before, and
each opens to read, or to delete; while they show, a question in the box starts a new one. A session
with any says how many in its header, beside its PRs and todo; a click there shows them above the
box (the only one, or the list), and another puts them away. What can be done with one is a row of
icons at the strip's top right (Earlier, Delete, Pop out, Close), each named on hover. Pop out moves
it into a window of its own beside the one it was in, which floats as a session kept on the desktop
does: the conversation fills it, its box asks only the side conversation, and the session's boxes go
back to the session, so you can talk to both at once (`/side` in the session's box still asks the
side conversation, and brings its window forward). Its top line drags it and its edges size it; Put
back closes the window and shows the conversation above the box again, and its × or ⌘W end it and
close the window. It comes back at launch where it was left, empty until you ask it something. Other
`/…` commands go to the session as typed.

#### The session panel

The selected session's panel keeps its top to three lines so the conversation has the room: the
title with the ways to open it, mark it seen, expand the panel, its details (ⓘ: account, ID, folder,
times) and its menu (⋯, New session here among the rest); where it stands, its worktree and branch,
and its account (its harness on hover); and the PRs it worked on (as the PRs view links them, else
its worktree's), each a chip that shows the PR in PRs on a click, opens it on GitHub on a
double-click and shows the PR's card on hover, with the todo
it's on and +N for the rest. Below is its whole conversation, read from the session's own record
(Claude Code's transcript, Codex's rollout, OpenCode's database): your prompts, the agent's replies
as Markdown, and every tool call, which opens to its input and output; runs of calls fold into one
line. Who said what and when heads a run of messages, not each one: your prompt, then the agent's
reply, whose later messages between its calls go on under that heading; it heads one again after a
note in the conversation or a pause of ten minutes (the agent woken by a background job's report). An artifact Claude Code published shows under its call as its title, a link that opens it,
with what the call did (Published · version 3), and stays out of a fold; web links in a tool's
output open too. A prompt shows the images you sent with it in place, whether you sent them from
Codealong, pasted them into the session's terminal or attached them in the Claude or Codex app;
click one to open it. It opens at the newest and loads older pages as you scroll up, and follows the
session as it goes. A link to a GitHub PR, in a reply or a prompt, reads `name#123` with the PR's
state as its icon; hovering it shows a card with where the PR stands (review, checks, branches,
size), the worktrees that hold it and the sessions that worked on it, each a click from its panel,
and its right-click menu is the PR's. A PR the board doesn't list (an older one, another repo's) is
asked of GitHub when its card first shows.

#### Moving through a conversation

⌘⇧↑ and ⌘⇧↓ bring your previous or next prompt to the top, from the box too (⌘↑ and ⌘↓ as well while
the cursor isn't in a text box), reading further back as they go; the prompt behind what you're
reading stays pinned at the top once it has scrolled off, two lines of it and more on hover, even
when a long turn put it pages back (they're read then, and kept for scrolling up), and a click
brings it up.

#### Find

⌘F finds words in it, wherever the panel shows: a bar over the conversation marks every match and
counts them across the whole conversation (the pages further back are read for it), starting from
the one nearest where you're reading; ⏎ goes to the match before, on into earlier pages, ⇧⏎ to the
one after (⌘G and ⇧⌘G from anywhere in the window), a run of tool calls opens to show a call it
finds, and Escape closes the bar and gives the keyboard back. It finds what shows: prompts, replies
and the calls' lines, and a call's input and output once you've opened it.

#### Copy and Run

A right-click on a message has Copy message, which copies it as it was written, Markdown and all,
and Copy for what you'd selected in it; in the peek, the notch, the waiting window and a kept
session, the session's own menu follows. Code blocks have Copy on hover, and a shell block in a reply (`bash`, `sh`, `zsh`…) or a command the
agent ran has Run beside it: that opens a new terminal where the session works, or where the command
ran, and types the commands there without their comments. A block with no language counts as shell
when each of its lines starts with a command on your PATH (`git`, `npm`, `cd`…), so output, a path or
a config file keeps Copy alone. A block led by `!`, which an agent writes for you to run in the
session itself, runs there instead: Run sends it to a Claude Code session in
its terminal or in the background as a reply, where Claude Code runs it in its shell and its agent
reads what it printed, and anywhere else opens a terminal for it without the `!`. A command run that
way, from Codealong or with `!` in the terminal, shows as yours with its output.

#### Replying

The box at the bottom sends it a message where it runs, as the waiting window does; paste a
screenshot or a file into it and the agent gets its path, and what you'd written to a session and
not sent stays with that session until you send it, in every window (the panel, the floating bar's
peek, the waiting window) and across restarts. A session whose last reply asks you something (Jev's
read) takes your answer there like any message. A Claude Code turn stopped before any reply takes
its prompt back, so the agent never sees it: the panel shows that prompt faint, marked "Taken back",
and puts it back in the box, as Claude Code does in its own. The next message Codealong types into
that terminal clears Claude Code's copy first (not in Terminal, which can't press a single key).

#### Approvals and questions

When a session waits on you to allow a tool call, the call shows above the box with Allow (⌘⏎) and
Deny (⌘⌫, with nothing written in the box, and the session stays shown so you can say what to do
instead): OpenCode's are answered through its service, Claude Code's through the Codealong plugin's
`PermissionRequest` hook when the account has the plugin (wherever the session runs: any terminal,
the Claude app), with Always too where Claude Code offers to allow calls like it from now on (its
button says what that adds), and otherwise by a keypress in its Ghostty, iTerm2 or tmux tab, only
while its own record says the prompt is up for that call. Questions a Claude Code session asks you
(its `AskUserQuestion`) show the same way through the plugin, one at a time under a strip of their
headers, each with its choices listed as Claude Code lists them, what each says about itself under it,
and a box for your own answer, and Answer sends your picks. Claude Code's own prompt stays up all the while, and
whichever answer comes first counts. Another session's message that a Claude Code session holds for
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
current turn).

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
comes round again after the rest. ⇧⌘F (or the button beside the check) expands the panel to the
whole window, the list and sidebar out of the way while you work through them, and stays so until
you press it again; Escape in the box then leaves the keys with the window.

#### The strip

A strip at the top of the panel says how many more wait on you, in one chip with each kind's glyph
and how many, and how the last one went. Pointing at the chip (or clicking it) lists what it counts
under it, kind by kind, the next first, each with where it is, as a folded pill on the bar lists its
sessions; a click on one shows it, as ⌘] would. It has what's commonly done to a session as buttons
that say their keys: Seen (⌘E), Done (⇧⌘E), Snooze (⌃⌘1 to ⌃⌘4, one for each of its menu's times: 15
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
(`p`), Reuse for new work and copies of their paths and branch names.

### PRs

Every PR in the project's repos, with the worktree and sessions that worked on each: the ones that
made, edited or commented on it, or pushed to it (from their own records: Claude Code's
`gitOperation` results and its `pr-link` lines, Codex's commands and attached PRs), the ones on its
branch while it was open, and any linked by hand (`codealong pr link|unlink`). One of them is the
PR's primary session, listed first and tagged Primary on its card and in its panel: the one whose
records show it opened the PR (the first to mark it), else the one that started first, unless
another was made it with `codealong pr nudge --session`.

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
changes), when it's approved, merged or closed, and when its checks fail, with where it stands
(ready to merge, its checks are still running), so it takes it from there. The message never asks
it to merge, an approval included: it says so, and the agent merges only if you asked it to. The
PR's other sessions (helpers, reviewers, ones that came to its branch) aren't told, nor is anyone
while the primary isn't running, and what you did to it yourself, which is what its agent did
through `gh`, isn't told back. Turned on for a PR that's approved already, it's told that at once
(not when the setting turns it on for every PR); what happened while Codealong wasn't running
isn't told, and a session that has stopped isn't resumed for it.

Settings › Automation › What a nudge tells it about (`[pull_requests] nudge_about`) narrows that,
for every PR with its nudge on: `"everything"` (the default) is all of the above; `"feedback"` is
only what asks for work, someone else's comments and reviews that ask for changes or say
something, so the session can work their feedback in; `"feedback-and-checks"` adds failing checks.
With either, an approval (its own words with it), a dismissed review, a merge and a close aren't
told, nor anything once the PR has ended, and the PR's card says so under its switch.

#### Automatic reviews

With `[reviews] auto = true` (Settings › Automation), an agent reviews each open PR that asks for your
review, isn't yours and isn't a draft, in a session of its own on `[reviews] account` and `model`
(else the account Auto would start, on its own model), apart from the account new sessions start
on. It runs out of sight: under Claude Code's background service, else in Codealong, else in a
terminal behind your windows. It starts in the worktree that holds the PR's branch, else the repo's
main checkout, and is handed the PR and its whole diff as files, each line of the diff numbered by
its place in the file's new version (so it never stops on a permission prompt to ask `gh`, and can
name its lines). What it's told to do is `[reviews] prompt`, which Settings shows to edit, filled
with Codealong's own instructions until you change it: what to look for and how, and, if you like,
to start subagents of its own (in Claude Code, ask for the Task tool and name each one's model) and
combine what they find. Either way Codealong tells it which PR, where it's written out, to change
and post nothing, and how to lay out its last message: a summary, then its comments on particular
lines. The session is linked to the PR, and is a helper of it (see *Helpers*): out of Sessions,
the bar and the queues while it doesn't need you, its turns never Your turn.

Review with… on a PR (its panel's header, its menu, its card's strip, ⌃⌘R) starts the same review
by hand, whatever `[reviews] auto` says: first the reviewer Settings names (Auto's account when it
names none), then each account that can run one out of sight, where it would run. It's a round like
any other, on the PR's card and in its Review tab, its review posted where `[reviews] post` says and
submitted as `[reviews] submit` does; it reviews a commit reviewed already too, a second review going
into your draft after the first. That account's session for the PR, when it has one, is asked again.
One that can't start says why where you asked for it. So Settings › Automation shows the reviewer,
where reviews go and what's submitted with automatic reviews off as well. The Codex app's own review
page is offered last, apart from Codealong's.

When its turn ends, its review goes where `[reviews] post` says. By default (`"draft"`) it's a
review on your GitHub account left as a draft that only you see, each comment on its line where the
PR's diff shows it (the rest written into the text, each under a link to its line), added after
what you wrote when you have a draft on that PR already: you finish it in the PR's Review tab or on
GitHub, approving or asking for changes, and the review stays asked of you until you submit it. `"comment"` posts a
comment from your account instead, each line comment under a link to its line, and `"none"` leaves
the review in the session. Never a submitted review, which would settle the request to you. What
goes on GitHub says an agent wrote it and which commit it read. When new commits come in while the
PR still waits on you, the same session reviews it again once the branch has held still five
minutes, told what changed since and asked what of its findings is fixed. Two run at once at most;
a session waiting on you holds no place. Until the agent's review of the PR's latest commit is done,
or didn't get done (two hours at most), the PR is out of what waits on you (the floating bar, the
notch, the waiting window and the mark), as a snoozed review is, so you review it after the agent;
it stays under Review requested, saying Agent reviewing, and its Review tab has a dot. The PR's
Review tab, in its panel and in its card in the notch and the waiting window, says where its latest
round stands, with the session that wrote it a click away, and shows your draft as GitHub has it
(see *Your review*); without a draft (a comment, or one you submitted) it shows the review the agent
wrote: its text, then each comment on a line of the code with the lines around it from the PR's
diff, its file and line opening that line on GitHub at the commit it read, and the reviews of
earlier commits too. Once you've submitted the agent's draft, it says so and how. `codealong
--snapshot --review-plan <owner/name> <number>` prints what a review would start, without starting
it, and `codealong --snapshot --pr-draft <owner/name> <number>` your draft as the Review tab reads
it.

#### Reviews of your own pull requests

With `[reviews.own] auto = true` (Settings › Automation, Review the pull requests your sessions
open), each open PR of yours that a session here opened (its primary, see *Nudging a PR's primary
session*) is reviewed the same way, by an agent on `[reviews.own] account` and `model`, else on the
account Auto would start among those of another harness than that session's, so another model reads
what one wrote (Codex reviews what Claude Code wrote, say); one of the same harness only when no
other can start. Drafts aren't reviewed, nor a PR whose session is gone or archived. Its
instructions are `[reviews.own] prompt`, else Codealong's own; it's told the review goes back to the
agent that wrote the PR, and to end on whether it would approve it (see *Your review*). Nothing goes
on GitHub: the review is kept in Codealong, in the PR's Review tab, its comments on lines with the
code around them. With `[reviews.own] send` (on unless turned off), the session that opened the PR
is sent a message once it runs here and doesn't wait on you to answer something: which agent
reviewed which commit, that its review is in a file it reads (kept as an attachment, so another
agent's words never reach it typed in as yours), to fix what holds up and say what it leaves, and
that it isn't asked to merge. A review that found nothing to raise isn't sent, nor one of a commit
the PR has moved on from: the next round's is. A send that doesn't go is tried a few times and then
said. The Review tab says where each review went or why it hasn't, and Send hands the latest to the
session by hand while it runs here, whatever `send` says. New commits are reviewed again once the
branch holds still, as above, so a session that fixes what was raised has its fixes looked at.

#### Your review

A PR's Review tab is your review of it. A draft review of yours on GitHub, the agent's or one you
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
shows the Review tab. Every agent review ends on what the agent would do with the PR, a `Verdict:`
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
repo whose Review tab has nothing to show says them there once, with Use them and a cross that
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
now. Rows that are your move are marked at the left edge: amber for a review asked of you or
something to fix, red for failing checks, green for your PRs that are ready to merge. Selecting a PR
shows a short header: its number and title, one line of its repo, branch, author and last update,
its pills and size, and the worktrees and sessions behind it on this Mac as chips (a worktree's
chip, then its sessions', each named by its title with its account on hover, the PR's primary
session marked; a click on a session shows it in the panel, on a worktree shows it in Worktrees,
and a right-click gives either's menu). A PR's row names one of its sessions the same way, its
primary one first, and how many more; a todo's row names up to three. The chevron
beside the title opens its details, and they stay open for the next PR until you close them: the
whole title, its repo, branch, author, when it was opened and its commits, who reviewed it and who's
asked to, and the chips as a tree. ⇧⌘F (or the button beside the chevron) expands the panel to the
whole window, as a session's does in Sessions, for reading its description and diffs at the
window's width. Under the merge box are its parts as tabs,
which stay at the top as what's under them scrolls (← → between them): Description, its description
then its comments (with `[jev] pull_requests`, the ones asking the author for a change or an answer
are marked and listed first); Changes, its changed files with their diffs (each file's name, then
its folder; a filter beside Files changed, which the file count goes to and which stays under the
tabs as the list scrolls, narrows them to the paths holding every word typed, `-word` leaving some
out, then lists the files whose changes mention the words, which are marked in the paths and diffs;
↑ ↓ and ⏎ open one, Escape clears it, ⌘F comes back to it on a PR's card); and Review (see *Your
review*). A PR opens on the tab it showed last, else on Review when the agent's review is there to
read and the PR waits on yours, else on Description. The tree under On this Mac says whether the
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
and a status: Todo, In progress (an agent is on it) or Done. The list has In progress, Ready,
Waiting and Done. Add one in the box at the top (`n`), reorder by dragging or with ⌃⌘↑ ⌃⌘↓, mark one
done with `x`. A todo can wait on other todos (Waits on, in its panel): until they're done it's
under Waiting and shows what it's waiting on, and a dependency that would make a loop is refused.
One an agent is already on stays In progress. Mark a todo as waiting by hand (`w`, the right-click
menu, or type what it waits for in Waits on) when it waits on something outside the list, like a
person's answer or a release; it stays there until you stop it waiting, start it or finish it. Stop
waiting (`w` again) also drops what it waits on that isn't done, so it's Ready at once; the toast
can undo it. Start session (⌘N) opens the quick panel with the todo as the prompt, and the session
it starts is linked to the todo, which moves to In progress; one started in the Claude app is linked
when it shows up. ⌘-click (or ⇧-click, ⇧↑ ⇧↓) picks several todos: Start one session (⌘N) starts a
single session on a prompt that numbers each of them, linked to all of them, and `x`, `w` and Delete
act on all of them.

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
chips you click. Nothing is asked twice: what was asked is kept in `jev-todos.json`.

### Canvas

The project's work laid out in space, to remember by where it is. Worktrees, sessions, todos and PRs
are compact cards (status, title, where it stands); rest the pointer on one for a popover with the
rest (move onto it to select its text, click its PR chip to select the PR's card, or show the PR in
PRs when the canvas has no card of it, and double-click the chip to open it on GitHub, click a
session in it to go
to that session, right-click one for its menu), select one for its panel on the right (a session's
shows its whole conversation and takes messages; the rest are the ones their own views show), type
in the filter at the top or pick a status in the sidebar to fade what doesn't match (a status, or
Return in the filter, also zooms to what does), double-click to open it where it lives, right-click
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

#### Controls

Controls follow Figma: scroll or Space-drag to pan, pinch or ⌘-scroll to zoom, ⇧1 fit all, ⇧2 the
selection, ⇧0 100%. Zoomed out, cards become blocks in their status color drawn by the GPU (WebGL2).

### Sidebar

Picks the view (Sessions, Worktrees, PRs, Todo, Canvas) and narrows it: to the devices you pick, to
what was active in the last 24h, 3d or 7d, and to the statuses you pick (Needs you, Your turn,
Working, In review, PR needs work; any of them). In Sessions it also sorts, by status or by when
sessions started. Devices and statuses toggle; none picked shows all, and Clear unpicks them. Save
the view as it is under a name to come back to it; right-click a saved view to update or delete it
(they're kept per Mac, not in config.toml). ⌃⌘S, or the button beside the traffic lights, folds it
to a rail of the views and devices whose filter button carries a badge while a filter narrows the
view. The search at the top narrows every view too.

### Notices

What stops working in the background (config.toml that doesn't read, GitHub signed out or out of
requests, a folder macOS is asking about) is a chip at the end of the accounts bar, in a few words.
A click shows the whole of it, when it ends where that's known (GitHub's rate limit says until
when), and what to open to fix it; its button dismisses it until it happens again. Several are one
chip that says how many.

### Tips

While a window waits on what it reads, a tip on using Codealong shows under what it's reading: the
main window before its first read of your sessions, the waiting window and the quick panel at
launch, a conversation or a PR's description that takes a moment to come. Each place has tips of
its own (the waiting window's are about its keys, the quick panel's about its own), with the keys
as they are now and the shortcuts as Settings has them; a tip about a shortcut that's off doesn't
show. At launch the tip comes with the line over it; a conversation or a description gets one only
once it has taken a moment, since they often come at once. Another comes every 12 seconds while the
wait goes on, and Next tip brings one at once. They come in turn, the ones you haven't seen first,
wherever you saw the last.

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
Dock icon or the bar brings it back, and ⌘Q quits.

### Settings window

*⌘, (comma), the app menu, or the foot of the sidebar*

A form for everything in config.toml. General has the folders searched for repos, the quick panel's
shortcut, the terminal and editor, and the theme; Sounds has the sound for each thing that starts to
wait on you, and how loud (see *Sounds*); Jev has the TypeSafe key (kept in the keychain)
and a switch per feature, each saying what it sends; Accounts adds, edits, removes and signs in
accounts, and under Found on this Mac lists the ones Codealong could read but doesn't, to add with a
click (with the agent skill, unless you untick it); Automation has what Codealong does by itself:
the agent that reviews the pull requests that wait on your review (the account and model it runs on
and what it's told, in a box like a session's, and where its review goes), nudging the session that
opened a pull request, moving a session that hits its usage limit, and telling one a lost connection
stopped to carry on, everywhere or for one repo with its team's settings (see *Settings for one
repo, and its team's*); Projects groups repos by pattern; Advanced turns
the performance log on and shows where the files are. Each change is saved in its place in the file,
so comments and order stay as you wrote them. The config.toml section edits the file itself, for
anything the form doesn't cover, and says what's wrong when it doesn't parse.

## Finding and starting work

### Quick panel

*⌥⌘/ from any app, over full-screen ones too; ⌘N, the title bar, the inspector, or a worktree's or
session's right-click menu in Codealong*

One input, and below it what you can do next: it finds and starts, and answering is the waiting
window's (below). With nothing typed it lists the sessions that need you, the longest wait first as
on the floating bar, then the PRs waiting on your review under Review requested, the longest wait
first, then the sessions whose turn ended (the working ones when nothing waits on you), and under
them the first three todos ready to start, in your order. Typed words find sessions by title,
account, worktree, branch, repo, project or PR, then by what was said in them (your prompts and the
agents' replies, for sessions active in the last `[search] days`, with the matching line shown; the
words as you typed them rank first), six at first and the rest a row away, and under them todos by
title, description, label, project, or the worktrees and sessions linked to them, then PRs by
number, title, repo, branch or author. A word of one or two letters among others counts only as a
whole word, so "class c" doesn't find everything with a c in it. Archived sessions are left out
unless you ask for them: the Archived box at the end of the Sessions heading, there whenever you
search, says how many match, and checked (a click, or ⇧⌘A) lists them after the others, each with an
Archived chip, in this and later searches until you uncheck it. ⏎ on a session shows it in
Codealong, with its conversation and a box to answer it, and ⌥⏎ opens it where it lives. ⏎ on a todo
shows its session the same way, or, when it has none, gets a session ready to start for it (its
prompt, its worktree, linked to it); on a PR it shows the PR in Codealong, and ⌥⏎ opens it on
GitHub; the PR's right-click menu copies its link. ⏎ or ⇥ on a PR under Review requested reviews it
in the waiting window, and ⇥ on a session answers it there; the panel puts itself away. The last row
starts a new session with what you typed as its prompt. ⏎ does the selected row, ⌘⏎ always starts,
⌥⏎ shows a todo or a PR under Review requested in Codealong, ⌘1 to ⌘9 do a row, Escape closes. Each
open starts empty, so ⏎ goes to what needs you; ↑ brings back what you'd typed before closing, then
earlier prompts you sent, started or saved, and ↓ goes forward again. While you type, the list
doesn't shrink until you pause and the answers are in, so the panel holds still. A project picker
(⌘P) narrows everything to one project. The shortcut is yours to change in Settings › General;
Settings names shortcuts macOS and common launchers already use.

### Suggestions in the quick panel

*Optional; Settings › Jev, `[jev] quick_panel` in config.toml*

Once you pause typing, TypeSafe's Jev reads what you wrote against the project's sessions and
worktrees and the panel leads with up to three suggestions, each with how likely it is: send it to a
session that's already going, start new work in a worktree (a new one when it's a change of its own)
or, for a question that needs none of your repos, in a chat of its own, open a session, or keep it
as a todo. ⏎ does the first. The same call says whether a running session may already be doing it
(start rows say so, and sending to it is offered), and puts the sessions it thinks you mean first
among the search results. A session that's running leads one that isn't when Jev finds them close (a
stopped one counts half against running ones), since sending to it doesn't resume anything. Jev
never picks the model: a new session starts on the one the start menu shows. The API key stays in
the login keychain. What you type, repo names and session titles and latest lines go to TypeSafe.

#### How a prompt gets to a session

Sending goes the way each harness takes a prompt: pasted into the session's Ghostty, iTerm2,
Terminal or tmux tab, Claude Code's daemon for a background job, OpenCode's service, a turn started
by the Codex app itself (opening the thread there first when no window has it; when the app doesn't
take it, nothing is sent: the message goes back in the box with why, and the app opens the thread
with it filled in to send from there), and Claude Code's cross-session socket for Claude app
sessions, which read it as a message from another session (what you type in Codealong says it's
yours, so one that skips permission prompts takes it without asking). A terminal session whose tab
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
commits), never goes as yours. It starts with a line saying Codealong sent it by itself and that its
user didn't write it, and goes the way another session's message does where the harness has one:
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

#### Switching models

A running session's box says the model it's on beside Send (the one its latest reply came from, or
that it was switched to since), and where Codealong types into its terminal or runs it, that's a
menu of its account's models: picking one switches that session alone (`/model <name>` typed into a
Claude Code session, its agent's model for one Codealong runs), between turns.

### New session

**New session**, the panel's last row: pick the worktree (⌘T; it starts at whatever's selected in
Codealong: a worktree, a session's, a PR's, or for a PR with none here its repo's main checkout;
pinned ones first), the account and model in one menu (⌘L, every account on every model it can start
on, with its limits, and the effort under them: Default, Low, Medium, High, Extra or Max, ← → to
change it), and where it runs (⌘O, a chip beside the agent menu that says the place, with a line on
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
the prompt, on the account's default model; without a prompt it opens the app's new-thread page.
OpenCode starts in your terminal on your prompt (`--prompt`), and so does the Codex CLI, for any
Codex account (`codex -- <prompt>`, images with `-i`), and Antigravity's command line, `agy`, once
it's installed (`agy -i <prompt>`, on one of the models it lists); the Antigravity app starts
conversations only in its own window.

#### In Codealong

**In Codealong** runs the session in Codealong itself, for any Claude Code, Codex or OpenCode
account, through the agent's Agent Client Protocol adapter (`codealong install` adds the ones for
Claude Code and Codex; OpenCode has its own): its panel shows the reply as it's written, takes the
next prompt (queued while a turn runs), answers what it asks to run with Allow and Deny, and Stop
ends a turn, as it does for sessions running elsewhere. The agent keeps the session in its usual
records, so it can be resumed in a terminal too, and after Codealong restarts, sending to it picks
it up again.

#### Auto

Auto, the default (and `codealong start`'s), keeps to preferred accounts (starred in Settings ›
Accounts; without a star, those of the first account's agent) while one has room, judged by the
limits its default model spends, where 90% of a limit is no room; the others only once they're all
out. Among them it takes the one whose weekly limit resets soonest, so no week's room goes unused,
else the one with the most room. It starts on that default model, or the next best (Opus, then
Sonnet) when the default's own weekly limit is nearly used up. Codealong only launches: the session
shows on the board like any other, and the panel stays on whatever you're looking at when it
appears.

#### Files in the prompt

Paste or drop screenshots and other files into the prompt: Claude Code in a terminal gets them as
file mentions (attached when they're 256 KB or less, read otherwise, with their folders added so it
doesn't ask), and the apps and OpenCode get their paths.

#### Chats

**No project**, last in the worktree picker, starts a chat for a question that belongs to none of
your repos: the session gets a folder of its own under `~/Library/Application
Support/Codealong/chats` (trusted once for Claude Code, for all of them), and no new worktree. The
chats are listed together in one lane, Chats, in Elsewhere, and `codealong start --chat` starts one.

### Skills by name

Type `/` and the start of a name, first or after any words. In the quick panel that lists the skills
of the project's repos (their `.claude/skills`, `.claude/commands`, `.agents/skills` and
`.codex/skills`), the most run first; ⏎ or ⇥ finishes the name, and the skill's repo is where the
session starts, in a worktree chosen as new work's is, with the rest of the line as its prompt, and
every skill named in it is written for the agent. A skill several repos have is a row for each. Only
accounts whose agent reads that skill's folder are offered, and the start row says which are left
out. In a session's reply box the list has Codealong's own commands, then the skills of the
session's worktree and of its account (plugins' included): ↑ ↓ choose, ⇥ finishes the name, Escape
puts the list away. The name goes as the agent takes it: `/name` to Claude Code, `$name` to Codex.
OpenCode and Antigravity can't be told to run a skill by name in a prompt, so they're offered none,
and another Mac's skills aren't listed yet.

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
and how many are left. Once one is sent a message, allowed, reviewed, marked seen, snoozed or
archived, the next that waits on you takes its place. ⌘] or ⌥⇧↓ skips to the next (a skipped one
comes round again after the rest) and ⌘[ or ⌥⇧↑ goes back, ⌥↑ ⌥↓ step along what waits, and ⌃- ⌃⇧-
go back and forward through the ones you looked at. A message that doesn't go is back in its
session's box, and that session comes round again. With nothing left it says nothing waits on you,
and the next that comes to wait on you shows by itself. Brought up again while it's open, it stays
where you were; closed and brought up again, it starts on the longest wait. Where it was is kept for
the next launch.

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
snoozed session leaves Needs you and Your turn, and the mark stops counting it; the snoozed pill
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
through the ones you looked at there. A strip over the header says how many more wait and how the
last one went, and has what's commonly done to the session as buttons that say their keys, as its
panel in Sessions has: Seen (⌘E), Done (⇧⌘E), Snooze (⌃⌘1 to ⌃⌘4), Close (⌃⌘W), Archive (⌃⌘A), Back
(⌃-), Next (⌘]) and × (esc). The PRs waiting on your review are in that same list, after the
sessions that need you and before the turns that ended, so the keys go from a session to a review
and on. On a review the strip has Approve (⌘⏎, with what's in the box as the review's text), Comment
(⌥⌘⏎, which needs text), Snooze (⌃⌘1 to ⌃⌘4), Review with (⌃⌘R starts one on the reviewer Settings names, else Auto's; the
button picks) and GitHub (⌃⌘G); Request changes is a button in the box, with no key, and needs text
too. Any of those settles GitHub's request to you, so the PR leaves what waits on you and the next
takes its place at once, the review going to GitHub behind it; one GitHub refuses comes back round
with what you wrote, and the strip says why. What you wrote stays with the PR until it goes, so
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
or away from the screen: Glass when a session needs you (it asks you something, waits on a call to
allow, or stops with an error), Pop when a session's turn ends, and, once you pick one, a sound when
a pull request asks for your review. Each can be one of macOS's sounds, one you put in
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

The session's own terminal tab (Ghostty, iTerm2 or Terminal, whichever it runs in), the Claude app
session, the Codex thread (a CLI thread's terminal, or `codex resume` in a new one), an OpenCode
session in its terminal (or resumed in a new one with `--session`), an Antigravity command line
conversation in its terminal (or resumed in a new one with `agy --conversation`), the Antigravity
app brought forward (it has no link to a conversation), a background job attached in a new terminal,
or the folder in your editor. A new terminal opens in the terminal app config.toml names, as a tab
in the window already showing that worktree, repo or project (`new_terminal` in config.toml;
Terminal can't open tabs for another app, so there it's a window). A worktree opens in your editor
(`editor` in config.toml, ⌘2) or, from Open in in its right-click menu and its Open button, any
other editor installed on the Mac: VS Code, Cursor, Windsurf, Zed, Sublime Text, Nova, BBEdit,
Xcode, the JetBrains IDEs and Android Studio, each with its app's icon. A session's right-click menu
and Open button, and cards pinned to the desktop, open its worktree's folder the same way, and ahead
of the editors, Open folder in your terminal: a new terminal in the session's own folder (the one it
was started in when that's inside its worktree, else the worktree's), placed as any new terminal is.
Picking another editor there makes it yours, as Codex's Open button remembers its last. A session's
tab that comes forward on another screen than the pointer's is outlined for a moment, and
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

## What Codealong works out

### Session status from Jev

*Optional; Settings › Jev, `[jev] sessions`*

Once per turn, Jev reads the end of the last reply and the command a session runs or waits on. A
reply that asks you something turns *Your turn* into *Needs you*, with the question; a busy session
quiet on a long build stays *Working* instead of *stuck*; a command left running in the background
says whether it polls for something, runs a job, is a server or watcher left running (no longer
*Working*), or waits on you (a sign-in, a password). Rows show how far the work got (Landed, Pushed,
Committed, Not committed, Blocked) and how risky a command waiting for approval looks; a worktree
whose sessions all read as done says *Looks finished*. Answers are kept in insights.json, so a turn
is asked about once.

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
over its title, and shows the latest one that came of it under the title, with its status and a "+N
more" for the rest; a click shows one and a right-click gives its menu, so Pin to desktop there puts
its card beside the one you're on. The bar's peek and the waiting window have the same header, and
its card's popover on the canvas lists them too. On the canvas, hovering or selecting a session
draws a line from the session it came from and to the ones that came of it, and a session's card on
the desktop does the same to the cards of those sessions pinned on the same screen.

### Helpers

A helper is a session that works for an agent rather than for you: one an agent started with
`codealong start --helper` (a review of its diff, a check, a second opinion on another account or
model), and an automatic review's session, which helps its PR, and for a PR one of your sessions
opened, that session too, as the review goes back to it. Nobody is expected to talk to it,
so a turn it ends is idle rather than Your turn, and while it doesn't need you it's out of its
worktree's status, the floating bar, the notch, the waiting window, the quick panel's lists and the
mark. Started without `--in`, it runs out of sight, as automatic reviews do.

In Sessions a helper is listed under the session it works for while it works or waits on you: on
one line, set in, joined to the session by a thread from the session's status, and naming its
worktree only when it isn't the session's. The session's row has a chip with each of its
helpers' status glyphs, the most urgent first (three, then +N), whose tooltip has a line for each:
*Review of #204 · done 12m ago*, *Review the diff · working for 3m*. A double-click on the chip
lists all its helpers under it, finished ones too, until another double-click; Codealong remembers
that across launches. Its panel lists them all under its title, as *Its helper*. The helper's own panel
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

A helper's message to the session that started it (`codealong session send`) isn't held for
you to allow when that session is a Claude Code one that skips permission prompts: it's work the
session asked for, and would read anyway. That holds only when the start said which session it
made, as one in Codealong, in Claude Code's background service or in Claude Code in a terminal
does; a helper found afterwards as the next session in its worktree (Codex, OpenCode or
Antigravity started in a terminal) could be another session started about then, so its message
is held as any agent's is. So is an automatic review's, whose reply reaches the PR's opener only
as `[reviews.own] send` says, and that of a helper you made your own; a start another Mac asks
for makes no helper. A message from any other agent is held there, as before, unless the agent's
own session skips them too.

Mark as done on a session marks its helpers done too, as their work was for it: each ends as far
as Codealong can end it and is archived, and one still at work is left to finish, unless you
chose to stop the session's work too. *Make it my own* in a helper's menu makes it a session of
yours for good: listed, counted and waiting on you as any of yours, and no longer told of to the
session that started it. A helper whose session is gone from the board is a session of your own
again. Macs from before helpers show another Mac's helpers as sessions of their own.

### Usage

**Usage** for every account in a bar across the top: 5-hour and weekly limits, per-model limits such
as Fable, and time to reset. When they don't all fit, each account shows the limit it's closest to
running out of and the rest on hover, and the bar scrolls sideways. OpenCode (bring your own
provider) and Antigravity (quotas in the app) have none to show. Add accounts in Settings ›
Accounts.

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
it done. Devices find each other on your tailnet when Tailscale runs, and reach linked devices
anywhere through iroh; they talk directly, encrypted end to end, with no server of Codealong's in
between. You link a device once: on your tailnet, both show the same six digits and you say yes on
the other one; elsewhere, with a link code made on one device and entered on the other.

## For agents

The `codealong` command and a skill that tells an agent when to use it. `codealong whoami` says
which session runs it (found by its processes, or for the Codex app and OpenCode by the session's
own record of the command), the other agents running in its worktree, and its open todos (the
worktree's too, when no other agent runs there); a worktree's agents are the sessions running in it,
not every session that ever worked there; `codealong todo
add|start|done|update|move|wait|ready|link` work the todo list, dependencies, waits and order
included (`todo list --ready` is what can start now, `--waiting` what's under Waiting), and starting
or finishing a todo links the session that ran the command; `codealong sessions [find <words>]`,
`session show`, `worktrees` and `accounts --usage` answer what else is running, what it did, where
it lives, and which account has room; `codealong session open` brings the main window up on a
session's conversation; `codealong session transcript` reads a session's conversation whatever its
harness, `codealong sessions --waiting` lists what waits on you, and `codealong session suggest`
puts replies above a session's box for you to send with a click; `codealong session
stop|rename|fork` do what a session's menu does, and
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
folder (`~/.codex/skills`, `~/.config/opencode/skills`, `~/.gemini/config/skills`), and keeps them
current; `codealong install` does the same from a terminal. Settings › Agents is where each
account's plugin or skill shows. See *For agents* below.

## Files Codealong keeps

Everything lives in `~/Library/Application Support/Codealong/`, beside `config.toml`.

What you've seen (for Your turn), what you've pinned, snoozed and archived in Codealong are kept in
`state.json` next to it; whether the bar is on, floating or in the notch, and where you left it
floating, in `bar.json`; what's pinned to the desktop, and where each card was left, in
`desktop.json`; the sessions kept there from the peek, and their windows' frames, in `kept.json`;
each project's canvas in `canvas.json`; which session started or was forked into which (and which
started ones are its helpers), where the harness doesn't say, in `origins.json`; whether your
latest quick-panel starts in each repo went into a new worktree, in `starts.json`; sessions linked to PRs or unlinked from them by hand, the
PRs nudged or not and the sessions picked as their primary, in `pr-links.json`; worktrees marked
reusable or not in `worktrees.json`; where each PR's automatic reviews stand in `reviews.json`;
replies suggested for sessions, until the sessions move on, in `suggestions.json`.
Todos and labels are in `todos.json`, which is watched, so an agent or an editor can add to it.
Attachments are kept by their content: images in `images/`, other files in `files/<hash>/<name>`;
ones no todo uses are cleared after 30 days. The devices this one is linked with are in
`devices.json`, and what each last said in `device-feeds.json`; this device's key is in the login
keychain.

## What's next

The menu bar icon and popover, and notifications (the design system's *Build plan*).
