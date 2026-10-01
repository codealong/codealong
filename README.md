# Codealong

<img src="design/app-icon.svg" alt="Codealong" width="96" height="96" />

A Mac app that keeps track of every coding agent you have running: Claude Code on two
accounts (`claude`, `claude2`), the Codex app, OpenCode and Antigravity. It sorts them by
project, worktree, branch and PR, shows what each one is doing, and opens any session where
it lives: its tab in Ghostty, iTerm2 or Terminal, the Claude app, Codex, Antigravity, or the
folder in your editor (VS Code, Cursor, Zed, Xcode and others).

**Every agent runs in a worktree, and every worktree is a lane.**

The design lives in the Codealong design system (tokens, components, and the model,
surfaces, harness adapters, apps, architecture and build plan pages):
https://claude.ai/artifact/Eg7p4HkWgTVUR54RfYgUPc

## What it does

- **Five views** of the current project, each in a window of its own if you like (⌘⇧N). The
  project switcher (⌘P) shows each project's key: ⌥⌘1 to ⌥⌘9 switch projects, ⌥⌘0 shows them
  all. ⌃Tab and ⌃⇧Tab go through the views (`v` too, outside Sessions), ⌥↑ ⌥↓ move along a
  list as the arrows do, and ⌃⌘↑ ⌃⌘↓ reorder what's pinned. A session's or worktree's
  actions have letters (`p` pin, `a` archive, `r` rename, `e` mark seen) and chords that work
  in every view (⌃⌘P, ⌃⌘A, ⌃⌘R, ⌘E); the right-click menu shows them. A click on a row's
  chips selects the row, as a click anywhere on it does, and a double-click does the chip's own
  thing (its PR on GitHub, the session where it runs, its todo, its app) in place of opening
  the row; in a panel, a click does. The views:
  - **Sessions**: every session in one list, with no headings. Pinned ones come first in your
    order (drag or ⌃⌘↑ ⌃⌘↓), each with a pin under its age, set a little apart from the rest.
    The sidebar's Sort by orders the rest. Status puts Needs you, Your turn and Working first, as
    the floating bar orders them, then the ones you snoozed, the soonest back first, then the
    rest by the day they were last active, the one you last sent a prompt first. Started puts the
    newest session first, and each row's age then says when it started. A row leads with the session's title, then what it's doing or said last; one that
    needs you has a label beside its title for what it waits on (Asks you, Needs approval,
    Stuck, Error). A snoozed session's row says how long until it's back. Here letters alone
    do nothing, so a word typed on the list by mistake doesn't pin, archive or rename: the
    keys are the chords, and they work from the panel's box too. Sessions are pinned here,
    with ⌃⌘P or the right-click menu, and their pins show only here. Archive a session with
    ⌃⌘A (`a` elsewhere) or its right-click menu; Codex threads are archived in Codex itself.
    Close
    terminal, in the right-click menu of a session running in a Ghostty, iTerm2 or Terminal
    tab, closes that tab whatever the session is doing, and End background job stops a Claude
    Code background job; either way its conversation is kept and resumes. Rename a session
    with ⌃⌘R (`r` elsewhere) or its right-click menu, here or wherever a session has one
    (the floating bar's pills, cards on the desktop and the quick panel's rows change the name
    in place). The name
    goes where its harness keeps names, so the harness shows it too: a Claude Code session's
    transcript, which `/resume` lists it by and a running session takes up, in its terminal's
    title too, the next time it reads its transcript's end; a Codex thread, which the Codex app
    shows the next time it reads its threads (not an archived one); an OpenCode session,
    through its service, so OpenCode must be running. Antigravity's can't be renamed here.
    Stop turn, in the right-click menu of a session whose turn is running (here, on the
    floating bar and in the quick panel), ends that turn and leaves the session waiting for
    you, as its own Stop or Escape would: Escape pressed in its Ghostty, iTerm2 or tmux tab for
    Claude Code and the Codex CLI (through its account's background service for a Claude Code
    background job), the Codex app's own Stop for its threads, and OpenCode's service for
    OpenCode. Claude app sessions, Terminal tabs and Antigravity can't be stopped from
    Codealong. Mark a
    finished turn seen, which makes it idle and takes it off the floating bar, with ⌘E (`e`
    elsewhere), the
    check at the top of its panel or its right-click menu. Fork in terminal, in a Claude Code,
    Codex or OpenCode session's right-click menu, opens a copy of its conversation in a new
    terminal while it carries on (OpenCode's copy is made by its service, so it needs OpenCode
    running). `/fork` in the box at the bottom of its panel forks it without a window coming
    forward, and you stay on the session: a Claude Code copy is a background job, an OpenCode
    copy runs in its service, and a Codex copy runs In Codealong (over its ACP adapter).
    Whatever follows `/fork` is the copy's first prompt; with nothing, the copy waits for one.
    Other `/…` commands go to the session as typed. The selected session's panel keeps its top
    to three lines so the conversation has the room: the title with the ways to open it, mark it
    seen, expand the panel, its details (ⓘ: account, ID, folder, times) and its menu (⋯, New
    session here among the rest); where it stands, its harness, worktree and branch; and the PRs it
    worked on (as the PRs view links them, else its worktree's), each a chip that opens on GitHub
    and shows the PR's card on hover, with the todo it's on and +N for the rest. Below is its
    whole conversation, read from the session's own record (Claude Code's transcript, Codex's rollout, OpenCode's database): your prompts, the
    agent's replies as Markdown, and every tool call, which opens to its input and output; runs
    of calls fold into one line. It opens at the newest and loads older pages as you scroll up,
    and follows the session as it goes. A link to a GitHub PR, in a reply or a prompt, reads
    `name#123` with the PR's state as its icon; hovering it shows a card with where the PR stands
    (review, checks, branches, size), the worktrees that hold it and the sessions that worked on
    it, each a click from its panel, and its right-click menu is the PR's. A PR the board doesn't
    list (an older one, another repo's) is asked of GitHub when its card first shows. ⌘⇧↑ and ⌘⇧↓
    bring your previous or next prompt to the top, from the box too (⌘↑ and ⌘↓ as well while the
    cursor isn't in a text box), reading further back as they go; the prompt behind what you're
    reading stays pinned at the top once it has scrolled off, two lines of it and more on hover,
    even when a long turn put it pages back (they're read then, and kept for scrolling up), and a
    click brings it up. Code blocks have Copy on hover, and a shell block in a
    reply (`bash`, `sh`, `zsh`…) or a command the agent ran has Run beside it: that opens a new
    terminal where the session works, or where the command ran, and types the commands there
    without their comments. The box at the bottom sends it a message where it runs,
    as the quick panel does; paste a screenshot or a file into it and the agent gets its path,
    and what you'd written to a session and not sent stays with that session until you send
    it, in every window (the panel, the floating bar's peek, the quick panel) and across
    restarts. A session whose last reply asks you something (Jev's read) takes your answer there
    like any message. A Claude Code turn stopped before any reply takes its prompt back, so the
    agent never sees it: the panel shows that prompt faint, marked "Taken back", and puts it back
    in the box, as Claude Code does in its own. The next message Codealong types into that
    terminal clears Claude Code's copy first (not in Terminal, which can't press a single key). When a session waits on you to allow a tool call, the call shows above the
    box with Allow and Deny: OpenCode's are answered through its service, Claude
    Code's through the Codealong plugin's `PermissionRequest` hook when the account has the
    plugin (wherever the session runs: any terminal, the Claude app), with Always too where
    Claude Code offers to allow calls like it from now on (its button says what that adds),
    and otherwise by a keypress in its Ghostty, iTerm2 or tmux tab, only while its own record says the
    prompt is up for that call. Questions a Claude Code session asks you (its `AskUserQuestion`)
    show the same way through the plugin, with their choices and a box for your own answer, and
    Answer sends your picks. Claude Code's own prompt stays up all the while, and whichever
    answer comes first counts. Others open where they run to be answered. The box stays meanwhile,
    keeping what you'd written, and Send works again once the session isn't waiting (a session
    run In Codealong takes it at once, after its current turn). The box's send button sits
    inside it, at its corner, and its placeholder says how a message gets there: typed into its
    Ghostty, iTerm2, Terminal or tmux tab, pasted into its background job, as a message from
    Codealong, or through its app. While a turn runs that Codealong can stop, Escape in the box
    stops it, as it does in Claude Code and Codex (⌘. anywhere in the window too), and so does
    the button while the box is empty; it says Stopping until the turn has ended. To work through what waits on
    you, ⌘⏎ in the box sends and goes on to the next session listed that waits on you (Needs
    you the longest wait first, then Your turn), or, while the session waits on a call that can
    be allowed here, allows it and goes on; ⏎ sends and stays. ⌘E (or the check at the top
    of the panel) marks a finished turn seen and goes on, and the keys stay where they were,
    so ⌘E works down the list or from box to box. ⌘] or ⌥⇧↓ skips to the next, ⌥⇧↑ goes back
    to the one before, and a skipped one comes round again after the rest. ⇧⌘F (or the button
    beside the check) expands
    the panel to the whole window, the list and sidebar out of the way while you work through
    them, and stays so until you press it again; Escape in the box then leaves the keys with the
    window. A line at the top of the panel says how many
    more wait on you and how the last one went. Clicking a session puts the cursor in its box;
    ⌥↑ ⌥↓ from there move along with the cursor in the next one's box, the arrows on the list
    leave the keys with the list, and Escape in the box gives them back.
  - **Worktrees**: each worktree once, pinned ones first in your order, then the rest by
    status: Needs you, Working, Your turn, In review, Idle, Merged. A worktree's row has a
    line for each agent that needs you, is working or just finished, which you select, open
    or right-click as you would a row. On its left, the worktree's name over its repo and
    branch, and beside them where it stands against the default branch: lines added and removed
    (a dot for uncommitted files) over commits ahead and behind, dropped on a narrow board. The
    rest says what's happening, with its agents, todo, app and PR. An agent is listed under the worktree it's working
    in, where most of its latest commands ran and edits landed, even if it started somewhere
    else; resuming it still happens where it started. Sessions outside your projects are
    listed under Elsewhere, in the repo or folder each ran in, and show in All projects only
    while they need you. Chats you start without a folder in the Claude app, or outside a
    project in the Codex app, are one worktree for each app, Claude app chats and Codex chats,
    shown like your projects' worktrees. Needs you is a queue: the worktree and
    the agent that have waited longest come first, so newer ones can't keep an old one down
    (worktrees that need you only through snoozed agents come after). Needs you, Working and
    Your turn are about agents: a worktree whose PR has failing checks or changes requested
    stays in In review, and the sidebar's PR needs work lists it.
    Within the other statuses, the worktree and the agent you last sent a prompt come first,
    so rows hold still while agents work. Stuck
    agents (busy but silent for 30 minutes) show in Needs you with the command they're stuck
    on. The sidebar's statuses narrow each worktree to the agents they're about. Pin a worktree
    with `p` or the right-click menu of the worktree or one of its agents, and reorder pins
    by dragging or with ⌃⌘↑ ⌃⌘↓; a pinned worktree stays whatever the age filter says. Beside
    its line diff, a row shows how far the worktree has moved from the default branch on the
    remote (`origin/main`), whatever its branch tracks: `↑3` for commits it has that the remote
    doesn't, `↓42` for commits the remote has that it doesn't. Canvas cards show the same.
  - **PRs**: every PR in the project's repos, with the worktree and sessions that worked on
    each: the ones that made, edited or commented on it (from their own records), the ones on
    its branch while it was open, and any linked by hand (`codealong pr link|unlink`). Reviews asked of you come first, with one-click Approve.
    A PR's right-click menu, on its row here or on its number where a worktree or session shows
    it (and the worktree's panel), opens it on GitHub or in Codex and copies its link; a
    worktree's own menu copies its PR's link too. Colored pills say where
    each PR stands: its review, its checks, and conflicts or an out-of-date branch, or one
    green Ready to merge when GitHub would merge it now. Rows that are your move are marked
    at the left edge: amber for a review asked of you or something to fix, red for failing
    checks, green for your PRs that are ready to merge. Selecting a PR shows its
    description, reviews, checks, comments (with `[jev] pull_requests`, the ones asking the
    author for a change or an answer are marked and listed first) and changed files with their
    diffs, and under On this Mac
    the worktrees and sessions behind it, drawn as a small tree: whether the worktree
    has anything GitHub doesn't, and the agents that worked on the PR, including one that made
    the worktree from another folder. Clicking one of those sessions, here or in a todo's
    panel, shows it in the panel (its conversation, a box to answer it) with a link back;
    double-click or its arrow opens it where it runs.
  - **Todo**: work you mean to hand an agent. Each todo has a title, a description that can
    hold screenshots and any other files (paste, drop or attach them; files copied in the
    Finder paste too), colored labels, and a status: Todo, In
    progress (an agent is on it) or Done. The list has In progress, Ready, Waiting and Done.
    Add one in the box at the top (`n`), reorder by dragging or with ⌃⌘↑ ⌃⌘↓, mark one done
    with `x`. A todo can wait on other todos (Waits on, in its panel): until they're done it's
    under Waiting and shows what it's waiting on, and a dependency that would make a loop is
    refused. One an agent is already on stays In progress. Mark a todo as waiting by hand
    (`w`, the right-click menu, or type what it waits for in Waits on) when it waits on
    something outside the list, like a person's answer or a release; it stays there until
    you stop it waiting, start it or finish it. Stop waiting (`w` again) also drops what it
    waits on that isn't done, so it's Ready at once; the toast can undo it. Start session
    (⌘N) opens the quick panel with the todo as the prompt, and the session it starts
    is linked to the todo, which moves to In progress; one started in the Claude
    app is linked when it shows up. ⌘-click (or ⇧-click, ⇧↑ ⇧↓) picks several todos: Start
    one session (⌘N) starts a single session on a prompt that numbers each of them, linked to
    all of them, and `x`, `w` and Delete act on all of them.
    Link worktrees and sessions you already have from the todo, or from a session's or
    worktree's right-click menu (Add to todo). A session or worktree on a todo shows the
    todo's chip; double-click it (click it in a panel) to go to the todo.
    With Jev on for todos (Settings › Jev, `[jev] todos`), a session that starts outside a todo
    is linked to the open todo it's for when Jev is sure; a todo whose linked sessions ended
    their turns, or whose PR merged, shows Looks done when Jev reads it as finished (Mark done,
    or Not yet until something moves on); and a todo's panel suggests its project, labels and
    the todos it waits on as chips you click. Nothing is asked twice: what was asked is kept in
    `jev-todos.json`.
  - **Canvas**: the project's work laid out in space, to remember by where it is. Worktrees,
    sessions, todos and PRs are compact cards (status, title, where it stands); hover one
    for the rest, select one for its panel on the right (a session's shows its whole
    conversation and takes messages; the rest are the ones their own views show), type in the
    filter at the top or pick a status in the sidebar to fade what doesn't match (a status, or
    Return in the filter, also zooms to what does), double-click to open it where it lives, right-click for its usual
    menu. What's on it is set in two places. Tidy (its panel, or Tidy up in a right-click) lays the canvas out
    once, holding what its own settings say, each kind on or off: sessions active in the last
    1h to 7d or all (24h at first) and any that's working or needs you; open todos changed or
    worked on in the last 7 or 30 days, or all of them; recent worktrees even with nothing on
    them (off, a worktree comes on only to head what's there); and PRs, yours and ones waiting
    on your review, or every open one. It lays the cards out in columns, one per worktree, in
    a frame per repo or per day, takes off cards older than that, kept or not, which is how the
    sessions you never archived leave, and says first what would come off and on. Frames you
    drew or renamed stay, with what's in them. Bring in (the gear in the toolbar) has the same
    choices for new work: what moves on after the last tidy (a new prompt, a finished turn, a
    question for you, a changed todo) comes on by itself and lands in its frame beside what it
    belongs to (a session under its worktree, a worktree in the next free column of its repo's
    or the latest day's frame, else in one column beside the frames); the frame grows to hold
    it, and what's below or beside moves along whole. Nothing comes off by itself, and what you
    take off doesn't come back by itself. Add older things from Add in the toolbar, a
    right-click on the canvas, or Add to canvas in any row's menu. Notes (`n`) and frames (`f`)
    organize it; a frame carries what's in it, and grows to hold a card dropped over its edge.
    Clear canvas (in Tidy, or a right-click on the canvas) takes everything off; Bring in adds
    work back as it moves on, and a tidy brings back what its settings hold. Controls
    follow Figma: scroll or Space-drag to pan, pinch or ⌘-scroll to zoom, ⇧1 fit all, ⇧2 the
    selection, ⇧0 100%. Zoomed out, cards become blocks in their status color drawn by the
    GPU (WebGL2).
- **Open where it lives**: the session's own terminal tab (Ghostty, iTerm2 or Terminal, whichever
  it runs in), the Claude app session, the Codex
  thread (a CLI thread's terminal, or `codex resume` in a new one), an OpenCode session in its terminal (or resumed in a new one with `--session`),
  Antigravity brought forward (it has no link to a conversation), a background job attached
  in a new terminal, or the folder in your editor. A new terminal opens in the terminal app
  config.toml names, as a tab in the window already showing that worktree, repo or project
  (`new_terminal` in config.toml; Terminal can't open tabs for another app, so there it's a window).
  A worktree opens in your editor (`editor` in config.toml, ⌘2) or, from Open in in its
  right-click menu and its Open button, any other editor installed on the Mac: VS Code,
  Cursor, Windsurf, Zed, Sublime Text, Nova, BBEdit, Xcode, the JetBrains IDEs and Android
  Studio, each with its app's icon. A session's right-click menu and Open button, and cards
  pinned to the desktop, open its worktree's folder the same way. Picking another editor there
  makes it yours, as Codex's Open button remembers its last.
  A session's tab that comes forward on another screen than the pointer's is outlined for a
  moment, and `move_pointer` moves the pointer to it.
- **Previews**: a worktree's app opens in the browser (⌘5, a double-click on the chip on its row, the link under
  its folder in its panel and in its sessions' panels, or its right-click menu). Its URL is set there (Set preview URL…) or with `codealong preview set
  <url>`, which a repo's own dev scripts or instructions can run, since only they know where a
  dev proxy serves each worktree's app (`https://feature.app.test`). A server listening in the
  worktree's folder is linked to it: the one on the port given with the URL, the port a
  localhost URL names, or else the one whose front page is a page, not an API or a debugger.
  The chip is green while that server runs and outlined when it doesn't; the panel says which
  process serves it, for how long, its page's title, and the session whose command started it.
  A worktree with no URL set that serves a page shows it at `localhost`.
  A preview can open in a Chrome profile, with that profile's sign-in and cookies: a project's
  `chrome_profile` (Settings › Projects) covers its worktrees, and a worktree can pick its own
  in its Preview URL sheet or with `codealong preview set --chrome-profile <name>`. Profiles
  are named as Chrome's profile picker shows them, by the account signed in to them, or by
  folder, and are read from Chrome's `Local State`. Servers are found from
  the kernel's list of your processes' listening ports every 10 seconds (a few milliseconds for
  every worktree at once), each new one is asked for its front page once, and the kernel says
  the moment one exits, so nothing is polled over HTTP. Only worktrees in projects are looked
  at, and only on this device.
- **Chrome tabs**: a Claude Code session that drove Chrome through Claude in Chrome has Show in
  Chrome in its right-click menu and its panel's Open menu, naming the Chrome profile when
  Chrome has more than one. It selects the session's own tab in whichever window has it, which
  shows its tab group, and brings that window forward. Once its tabs are gone, the item reads
  Reopen <site> in Chrome and opens the session's last page again in that profile. The tabs
  come from the session's transcript (each of the extension's results lists the session's tab
  group); the profile from the extension's storage in each profile, which records its tab
  groups, or from each profile's History for a group it no longer remembers.
- **Reusable worktrees**: a worktree that takes one piece of work after another (its
  right-click menu's Reuse for new work, or `codealong reuse on`), where others are kept for
  one piece of work each. A repo's main worktree (named `main` on its row, as git calls it) and
  a worktree that has taken new work three times or more (another branch checked out, or its
  branch reset, as its HEAD's reflog says) are reusable unless turned off. Its row shows a
  repeat mark before its repo, and Free while it holds no work: nothing uncommitted or
  unlanded, and no agent at work (otherwise work under way, in review, or landed once its PR
  merged). Jev suggests new work only in reusable worktrees: the repo's free or landed ones
  first, joining the work under way in one only when Jev is sure the work belongs there, else
  the main worktree. A new worktree is suggested only in a repo where you often start in one
  from the quick panel (three or more, and half your latest starts there), and a repo named in
  what you type keeps other repos' worktrees out of the suggestions. When a reusable worktree
  takes new work, the sessions from before move to Earlier work in its panel, off its row and
  out of its status. Settings › Projects lists them, and suggests worktrees whose branch has
  carried several merged PRs.
- **Quick panel** (⌥⌘/ from any app, over full-screen ones too; ⌘N, the title bar, the
  inspector, or a worktree's or session's right-click menu in Codealong): one input, and below
  it what you can do next. With nothing typed it lists the sessions that need you, the
  longest wait first as on the floating bar, then the ones whose turn ended (the working ones when nothing waits on you), and under them the first
  three todos ready to start, in your order. Typed words find
  sessions by title, account, worktree, branch, repo, project or PR, then by what was said in
  them (your prompts and the agents' replies, for sessions active in the last `[search] days`,
  with the matching line shown; the words as you typed them rank first), six at first and the
  rest a row away, and under them todos by
  title, description, label, project, or the worktrees and sessions linked to them, then PRs by
  number, title, repo, branch or author. A word of one or two letters among others counts only
  as a whole word, so "class c" doesn't find everything with a c in it. ⏎ on a session
  shows it in Codealong, with its conversation and a box to answer it, and ⌥⏎ opens it where it
  lives. ⏎ on a todo
  shows its session the same way, or, when it has none, gets a session ready to start for it (its prompt,
  its worktree, linked to it); on a PR it opens the PR on GitHub, and the PR's right-click menu
  copies its link. The last row starts a new session with what you typed as its
  prompt. ⏎ does the selected row, ⌘⏎ always starts, ⌥⏎
  shows a todo or PR in Codealong, ⌘1 to ⌘9 do a row, Escape closes. ⇥ on a
  session answers it in the panel: its conversation, and a box that sends it a message where it
  runs (⏎ sends and closes the panel), or what it waits on you to allow, with Allow and Deny
  (⌘⏎ allows); Escape goes back to the list. Answered from the list with nothing typed, the
  panel works through what waits on you: once a session is sent a message, allowed, marked
  seen, snoozed or archived, the next one that waits on you takes its place, and a line at the
  top says how the last one went and how many are left. ⌘] skips to the next (a skipped one
  comes round again after the rest), and after the last one the panel closes. A message that
  doesn't go is back in its session's box, and that session comes round again. Each open starts empty,
  so ⏎ goes to what needs you; ↑ brings back what you'd typed before closing, then earlier prompts you sent, started or saved, and ↓ goes forward again. While you type, the list doesn't shrink until you pause and the answers are in, so the panel holds still. A project picker (⌘P) narrows everything to one project. The shortcut is
  yours to change in Settings › General; Settings names shortcuts macOS and common launchers
  already use.
- **Session status from Jev** (optional; Settings › Jev, `[jev] sessions`): once per turn,
  Jev reads the end of the last reply and the command a session runs or waits on. A reply
  that asks you something turns *Your turn* into *Needs you*, with the question; a busy session
  quiet on a long build stays *Working* instead of *stuck*; a command left running in the
  background says whether it polls for something, runs a job, is a server or watcher left
  running (no longer *Working*), or waits on you (a sign-in, a password). Rows show how far the work
  got (Landed, Pushed, Committed, Not committed, Blocked) and how risky a command waiting for
  approval looks; a worktree whose sessions all read as done says *Looks finished*. Answers are
  kept in insights.json, so a turn is asked about once.
- **Commands left running**: a Claude session whose turn left a command running in the
  background (registry status `shell`) is *Working* while that command does something. The
  engine looks at the command's processes once a minute; when none has started or used CPU for
  15 minutes (a dev server, a sign-in waiting on a browser, `tail -f`), the turn is over and the
  row says *Left running*.
- **Suggestions in the quick panel** (optional; Settings › Jev, `[jev] quick_panel` in config.toml):
  once you pause typing, TypeSafe's Jev reads what you wrote against the project's sessions and
  worktrees and the panel leads with up to three suggestions, each with how likely it is: send
  it to a session that's already going, start new work in a worktree (a new one when it's a
  change of its own), open a session, or keep it as a todo. ⏎ does the first. The same call
  says whether a running session may already be doing it (start rows say so, and sending to it
  is offered), and puts the sessions it thinks you mean first among the search results. A
  session that's running leads one that isn't when Jev finds them close (a stopped one counts
  half against running ones), since sending to it doesn't resume anything. Jev
  never picks the model: a new session starts on the one the start menu shows. Sending goes the
  way each harness takes a prompt: pasted into the session's Ghostty, iTerm2, Terminal or tmux tab, Claude Code's daemon
  for a background job, OpenCode's service, a turn started by the Codex app itself (opening
  the thread there first when no window has it; else it opens with the prompt filled in), and Claude Code's cross-session socket for Claude
  app sessions, which read it as a message from another session (what you type in Codealong
  says it's yours, so one that skips permission prompts takes it without asking). A terminal
  session whose tab can't be found gets such a message too, and the note (on the bar, from the
  peek) says it wasn't typed and why; once typing has started, nothing else is tried, so it
  can't arrive twice, and the same text sent again while it's on its way goes once. A session that isn't running
  is resumed with the prompt; one waiting on you is opened instead, so what you typed can't
  answer its question. The API key stays in the login keychain. What you type, repo names and
  session titles and latest lines go to TypeSafe.
- **New session**, the panel's last row: pick the worktree (⌘T; it starts at whatever's
  selected in Codealong: a worktree, a session's, a PR's, or for a PR with none here its repo's
  main checkout; pinned ones first), the account and model in one menu (⌘L, every account on
  every model it can start on, with its limits, and the effort under them: Default, Low,
  Medium, High, Extra or Max, ← → to change it), and where it runs. With other Macs linked,
  ⌘D picks the Mac too: its worktrees and accounts are listed, and the session starts there
  (it needs remote control on over there). Your terminal (Ghostty, iTerm2 or Terminal) starts Claude Code
  on your prompt, on the account's default model or the one you picked (Fable, Opus, Sonnet,
  Haiku, 1M context, Opus Plan Mode) at the effort you picked (`--effort`; Haiku takes none,
  and Default is the account's `effortLevel`), and with New worktree Claude Code makes a fresh worktree
  for it (`claude --worktree`: its folder, branch and base are Claude Code's choice).
  Codealong trusts the folder for that account first, as Yes at Claude Code's trust prompt does
  (the prompt's default answer quits, and `--bg` refuses a folder it hasn't trusted).
  Background starts it the same way under Claude Code's background service (`claude --bg`),
  with no terminal: it shows on the board like any other, takes prompts from Codealong, and
  Attach opens a terminal on it when you want one. The
  Claude app opens a new session with the prompt filled in, and you send it there (its own
  model and worktree choices included). The Codex app runs the new thread on your prompt:
  Codealong makes the thread, opens it in the app and has the app send the prompt, on the
  account's default model; without a prompt it opens the app's new-thread page. OpenCode starts in your terminal on your prompt
  (`--prompt`), and so does the Codex CLI, for any Codex account (`codex -- <prompt>`, images
  with `-i`); Antigravity starts conversations only in its own window. **In Codealong** runs
  the session in Codealong itself, for any Claude Code, Codex or OpenCode account, through the
  agent's Agent Client Protocol adapter (`codealong install` adds the ones for Claude Code and
  Codex; OpenCode has its own): its panel shows the reply as it's written, takes the next
  prompt (queued while a turn runs), answers what it asks to run with Allow and Deny, and Stop
  ends a turn, as it does for sessions running elsewhere. The agent keeps the session in its usual records, so it can be resumed in a
  terminal too, and after Codealong restarts, sending to it picks it up again. Auto, the default
  (and `codealong start`'s), keeps to preferred accounts (starred in Settings › Accounts;
  without a star, those of the first account's agent) while one has room, judged by the limits
  its default model spends, where 90% of a limit is no room; the others only once they're all
  out. Among them it takes the one whose weekly limit resets soonest, so no week's room goes
  unused, else the one with the most room. It starts on
  that default model, or the next best (Opus, then Sonnet) when the default's own weekly limit
  is nearly used up. Codealong only launches: the session shows on the board like any other,
  and the panel stays on whatever you're looking at when it appears. Paste or drop screenshots and
  other files into the prompt: Claude Code in a terminal gets them as file mentions (attached
  when they're 256 KB or less, read otherwise, with their folders added so it doesn't ask),
  and the apps and OpenCode get their paths.
- **Usage** for every account in a bar across the top: 5-hour and weekly limits, per-model
  limits such as Fable, and time to reset. When they don't all fit, each account shows the
  limit it's closest to running out of and the rest on hover, and the bar scrolls sideways.
  OpenCode (bring your own provider) and Antigravity (quotas in the app) have none to show.
  Add accounts in Settings › Accounts.
- **Settings** (⌘,, the app menu, or the foot of the sidebar): a form for everything in
  config.toml. General has the folders searched for repos, the quick panel's shortcut, the
  terminal and editor, and the theme; Jev has the TypeSafe key (kept in the keychain) and a
  switch per feature, each saying what it sends; Accounts adds, edits, removes and signs in accounts, and under Found on this Mac lists
  the ones Codealong could read but doesn't, to add with a click (with the agent skill, unless
  you untick it); Projects groups repos by pattern;
  Advanced turns the performance log on and shows where the files are. Each change is saved in
  its place in the file, so comments and order stay as you wrote them. The config.toml section
  edits the file itself, for anything the form doesn't cover, and says what's wrong when it
  doesn't parse.
- **Sidebar**: picks the view (Sessions, Worktrees, PRs, Todo, Canvas) and narrows it: to the
  devices you pick, to what was active in the last 24h, 3d or 7d, and to the statuses you pick
  (Needs you, Your turn, Working, In review, PR needs work; any of them). In Sessions it also
  sorts, by status or by when sessions started. Devices and statuses
  toggle; none picked shows all, and Clear unpicks them. Save the view as it is under a name to come back to it;
  right-click a saved view to update or delete it (they're kept per Mac, not in config.toml).
  ⌃⌘S, or the button beside the traffic lights, folds it to a rail of the views and devices
  whose filter button carries a badge while a filter narrows the view. The search at the top
  narrows every view too.
- **Light or dark**: follows macOS unless you pick light or dark with the buttons at the foot
  of the sidebar. The choice is saved in config.toml, and every window follows it.
- **Resizable panels**: drag the inner edge of the sidebar or the panel on the right, and
  double-click it to go back to the default width. The PR panel keeps a width of its own.
- **Floating bar**: a small bar on top of every window, on every Space and over full-screen
  apps, with a pill for each agent that needs you, whose turn just ended, or that's running,
  one for PRs waiting on your review, and one for todos ready to start. The project button
  beside the mark shows one project or all of them; a badge on it counts what needs you in the
  others. Rest the pointer on a session's pill and its peek opens under the bar: the session's
  conversation, as its panel shows it, and a box to answer it. The peek never takes the keyboard
  by itself, so passing over the bar can't take what you're typing elsewhere; click in it to
  type, and ⏎ sends where the session runs and puts it away (Escape does too, keeping what you
  wrote with the session). A session waiting on you to allow a tool call shows it with Allow
  and Deny above the box; its ⋯ button, or a right-click in it, has the session's menu, as
  everywhere else. Until you click in it, the peek follows the pointer from pill to pill
  and closes once the pointer leaves it; pills that shift under a still pointer as sessions
  change groups don't move it. Once you've clicked in it, it stays on its session whatever the
  bar does, until you send, press Escape or click somewhere else. Click a pill to show its
  session in Codealong, selected in Sessions, and double-click it to open the
  session where it lives; drag it off the bar (floating or in the notch) to pin its card to the
  desktop where you let go; right-click it for the session's menu, with Reply here to answer it in
  the peek and Rename… to rename it there. The todos pill opens the quick panel ready to start a
  session for a todo (its menu picks which).
  **Snooze** (a session's menu on the bar or in Codealong, the peek's zzz button, or a folded
  pill's menu for all of its sessions): for 15 minutes, an hour, three hours or until 9 the
  next morning. A snoozed session leaves Needs you and Your turn, and the mark stops counting
  it; the snoozed pill lists it with when it's back, to show it in Codealong, open it or wake it
  early. When the time
  comes it's back as it is then, and one with nothing new (its turn already seen, or its
  process gone) comes back as Your turn, so a snooze always brings its session back. A running
  session stays in Working while snoozed, and sending a session a prompt ends its snooze.
  Needs you and Working are queues, the longest first, so a new one joins the end instead of
  pushing an old one down; Your turn is the newest first. Pills that
  don't fit fold into one per group that lists them, and a session picked from that list, or
  from the recent pill's, shows in Codealong's window. The mark brings Codealong forward; drag
  the bar by it or by its gaps. **In the notch**, the bar docks at the top of the screen instead:
  one black shape with the notch, dark whatever the theme, showing the mark on the camera's
  left and, on its right, the most urgent group and how many (Needs you, then Your turn, then
  Working; a new one that needs you pulses). Rest the pointer on it for a moment, or click it,
  and it opens down into the bar's pills, the peek opening under them as on the floating bar;
  it closes a moment after the pointer leaves, and stays open while the peek or one of its
  menus is up. The pin at the end of its right side, or Keep the notch open in its right-click
  menu, keeps it open until you unpin it, across launches. A screen without a notch (an external display) gets one drawn at the top center
  of its menu bar. The pair of buttons at the top right of the window floats the bar or puts it
  in the notch, and picking the one that's on turns it off, as do Window › Floating bar and Bar
  in the notch and the bar's own right-click menu.
- **Pin to desktop** (a session's, worktree's or todo's right-click menu, in any view or on the
  canvas, or a session's pill dragged off the bar): puts its card, the one the Canvas view shows, on the desktop, floating over every
  window, on every Space and over full-screen apps as the floating bar does. A worktree's card
  lists its sessions that are running or waiting on you, and a todo's its sessions. Drag a card
  where you want it; it stays there across launches. Click a session (its card, a session line,
  or a todo's card) to open its peek beside the card, as the bar's pills do, with the keyboard in
  its box; click again to put it away. Double-click to open what it shows where it lives (a todo
  without a session starts one in the quick panel). Right-click it, or a session line in it, for
  that session's (Rename… renames it in its peek), worktree's or todo's menu, where Remove from
  desktop takes the card off; the menu in Codealong also offers that once it's pinned.
- **Where a session came from**: a fork of another session, or one another session's agent
  started. Codealong reads what the harnesses record (Claude Code's `/fork`, `/background` and
  forked background jobs; Codex's forks and the threads its app makes for an agent; OpenCode's
  forks) and notes what they don't: a session started by an agent's `codealong start`, and a
  fork into a terminal made from Codealong. A session's panel says what it was forked from or
  started by and what came of it, each a click away. On the canvas, hovering or selecting a
  session draws a line from the session it came from and to the ones that came of it, and a
  session's card on the desktop does the same to the cards of those sessions pinned there.
- **New window** (File › New window, ⌘⇧N): one more window on the project you're looking at.
  Closing the main window hides it; New window, the Dock icon or the bar brings it back, and
  ⌘Q quits.
- **Close idle terminals** (File › Close idle terminals…): lists the Ghostty, iTerm2 and Terminal tabs of sessions
  that sat idle for a day or more (12 hours to a week, you choose) and asked you nothing in
  their last reply, by Jev's read where it has one, else by it not ending in a question. It
  says why for each, and closes the ones you leave ticked. Nothing closes by itself. Closing
  ends the agent's process; the conversation stays and resumes. `codealong sessions close-idle
  --dry-run` lists the same from the command line.

- **Your other devices** (Settings › Devices; off until you turn it on): Codealong on each of
  your devices shows the others' worktrees and sessions beside its own. Everything from another
  device wears that device's badge, a colored square and its name (pick each device's color in
  Settings › Devices); other devices' worktrees go into this device's projects by their repos,
  whatever those devices call them, even where a checkout still has a renamed repo's old name;
  work on a device that's away stays, faded, as it was when last seen; the sidebar's Devices
  narrow the views to the ones you pick; and Worktrees can group by device. A session on another
  device shows its whole conversation in its panel, and when that device lets its linked
  devices act on its sessions (remote control, off by default) you can send it a message or
  answer an approval from here. Devices find each other on your tailnet when Tailscale runs,
  and reach linked devices anywhere through iroh; they talk directly, encrypted end to end,
  with no server of Codealong's in between. You link a device once: on your tailnet, both show
  the same six digits and you say yes on the other one; elsewhere, with a link code made on one
  device and entered on the other.

- **For agents**: the `codealong` command and a skill that tells an agent when to use it.
  `codealong whoami` says which session runs it (found by its processes, or for the Codex app
  and OpenCode by the session's own record of the command), the other agents running in its
  worktree, and its open todos (the worktree's too, when no other agent runs there); a worktree's
  agents are the sessions running in it, not every session that ever worked there;
  `codealong todo add|start|done|update|move|wait|ready|link`
  work the todo list, dependencies, waits and order included (`todo list --ready` is what can
  start now, `--waiting` what's under Waiting), and starting or finishing a todo links the
  session that ran the command; `codealong
  sessions [find <words>]`, `session show`, `worktrees` and `accounts --usage` answer what else
  is running, what it did, where it lives, and which account has room; `codealong session open`
  brings the main window up on a session's conversation; `codealong start
  --todo <id>` starts a session the way New session does (`--todo` again for several todos in
  one session); `codealong config show|check`
  prints the settings and checks an edited config.toml. With the app running, answers come
  from it over a socket and todo changes go through its engine; with it closed, the command
  reads the files (sessions then show without git or PRs). Output is JSON when piped, tables
  in a terminal. Settings › Agents installs the command (a symlink on PATH), the Codealong
  plugin in every Claude Code account (the skill, the command for its Bash tool, and the hooks
  that let you answer its permission prompts and questions from Codealong), and the skill in
  every other account's skills folder (`~/.codex/skills`, `~/.config/opencode/skills`,
  `~/.gemini/config/skills`), and keeps them current; `codealong install` does the same from a
  terminal. Settings › Agents is where each account's plugin or skill shows. See *For agents*
  below.

Next: the menu bar icon and popover, and notifications (the design system's *Build plan*).

## Requirements

- macOS 13 or later
- Rust (`rustup`), Node 20+
- `gh` signed in (`gh auth login`) for PRs
- A terminal: Ghostty 1.3+, iTerm2 or Terminal (a new config.toml picks the first of those installed)

## Develop

```bash
npm install
npm run tauri dev      # the app with hot reload
scripts/relaunch.sh    # build Codealong Dev, then quit and reopen it
scripts/release.sh     # the installer for other Macs (design/plans/releases.md)
npm run snapshot       # print the board as a table without opening a window
```

Builds from `relaunch.sh` are Codealong Dev: a light icon and an id of their own, so they never
pass for the Codealong installed in Applications. Both use your settings and todos; whichever you
open last quits the other, and the `codealong` command follows it.

Builds go to `codealong-target` next to the main checkout, one directory shared by every
checkout and worktree (`.cargo/config.toml`); `scripts/target-dir.sh` prints it and has
worktrees inside the checkout build there too. Tests: `npm test` (vitest),
`npm run typecheck`, and `cargo test` in `src-tauri`
(model rules, parsers, git and GitHub parsing, usage, config writes, deep links, terminal
scripts). `scripts/preview.sh` renders the UI in headless Chrome against your real data with
Tauri's IPC stubbed. CLAUDE.md has the rest of the working notes.

The [logo master and brand notes](design/brand/README.md) live in `design/brand`.
Run `npm run icons` to regenerate the UI mark, browser icon and native bundle icons.
The mark's inner node shows the app-wide status: red for session errors, amber when
you are needed (including PR review requests), blue for unread results, green while
agents work, and light gray when idle, in that priority order. The project switcher,
floating bar, browser icon and running Dock icon stay in sync across projects.

## Settings

`~/Library/Application Support/Codealong/config.toml`, written on first run from what
Codealong finds and watched for changes. It finds each harness by the files it keeps: Claude
Code's `~/.claude` and sibling folders like `~/.claude2`, `~/.codex`, OpenCode's and
Antigravity's databases, and folders your shell's wrappers and aliases point
`CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `XDG_DATA_HOME` at, which also name the account and start
it. The ones in use or signed in go into config.toml; the rest wait under Settings › Accounts ›
Found on this Mac. Settings (⌘,) edits it as a form, or as text:

```toml
scan = ["~/projects"]          # where repos are found, three levels deep

[apps]
terminal = "ghostty"           # or "iterm2", "terminal"
editor = "cursor"              # or "vscode", "zed", "xcode"… (Settings lists those installed), or an app's name
new_terminal = "project"       # a tab beside the same worktree, repo or project; or "tab", "window"
move_pointer = false           # move the pointer to a terminal brought forward on another screen

[appearance]
theme = "system"               # follows macOS; or "light", "dark"

[shortcuts]
quick_panel = "Option+Cmd+/"        # works in every app; "" turns it off (Settings › General)

[jev]                          # TypeSafe's Jev, each feature off until turned on (Settings › Jev)
model = "jev-latest"
quick_panel = false            # suggestions as you type in the quick panel
sessions = false               # how each turn ended: asks you, stuck, landed, finished, risk
todos = false                  # links sessions to todos, done hints, new todos' labels
pull_requests = false          # review comments that ask for a change

[search]
days = 14                      # prompts and replies searched this far back; 0 is titles only (Settings › General)

[devices]                      # your other devices (Settings › Devices)
enabled = false
tailscale = true               # find them on your tailnet
iroh = true                    # reach linked devices anywhere
remote_control = false         # let linked devices act on sessions here

[debug]
perf_log = false               # true logs timings to ~/Library/Logs/Codealong/perf.log (Settings › Advanced)

[[accounts]]
name = "claude2"               # what the board shows
harness = "claude"             # claude, codex, opencode or antigravity
home = "~/.claude2"
command = "claude2"            # used for `claude2 --resume <id>`
start = "background"           # where new sessions start: terminal, background, claude-app (~/.claude only), codex-app (~/.codex only), codealong
# acp = "my-adapter --stdio"   # the Agent Client Protocol adapter Codealong runs its sessions through, in place of the installed one
preferred = true               # Auto starts it over accounts without this while it has room

[[accounts]]
name = "opencode"
harness = "opencode"
home = "~/.local/share/opencode"  # OpenCode's data folder, with opencode.db
command = "opencode"           # used for `opencode --session <id>`

[[accounts]]
name = "antigravity"
harness = "antigravity"
home = "~/.gemini/antigravity" # its app data folder, with conversation_summaries.db

[[projects]]
name = "Northwind"
repos = ["github.com/northwindai/*"]
folders = []
chrome_profile = "Work"        # its worktrees' previews open in this Chrome profile; without it, the default browser
```

What you've seen (for Your turn), what you've pinned, snoozed and archived in Codealong are
kept in `state.json` next to it; whether the bar is on, floating or in the notch, and where you
left it floating, in `bar.json`; what's pinned to the desktop, and where each card was left, in `desktop.json`; each project's canvas in `canvas.json`; which session started or was forked into which, where the harness doesn't say, in `origins.json`; whether your latest quick-panel starts in each repo went into a new worktree, in `starts.json`; sessions linked to PRs or unlinked from
them by hand in `pr-links.json`; worktrees marked reusable or not in `worktrees.json`. Todos and labels are in `todos.json`, which is watched, so an
agent or an editor can add to it. Attachments are kept by their content: images in `images/`,
other files in `files/<hash>/<name>`; ones no todo uses are cleared after 30 days. The devices
this one is linked with are in `devices.json`, and what each last said in `device-feeds.json`;
this device's key is in the login keychain.

## For agents

`skills/codealong/` is one skill in the Agent Skills layout (`SKILL.md` plus
`references/reference.md`, generated from `codealong --help` by `scripts/skill-reference.sh`),
read as-is by Claude Code, Codex, OpenCode and Antigravity. The app ships it in its bundle and
Settings › Agents (or `codealong install`, or adding an account in Settings) copies it into the
skills folder of each account that isn't Claude Code, with a `.codealong-version` marker; at
launch the app refreshes copies it made when the skill changed, including ones in folders of
accounts you've since removed, and never touches a folder it didn't make. A copy someone edited
is left alone: Settings › Agents marks it Edited, and replaces it only when asked (so does
`codealong install --account <name>`).

`plugin/` wraps the same skill as a Claude Code plugin (`.claude-plugin/plugin.json`, a
`bin/codealong` shim that finds the app, a `SessionStart` hook that runs `codealong whoami
--context`, and a `PermissionRequest` hook that runs the hidden `codealong session hook
permission-request`), and `.claude-plugin/marketplace.json` makes the repo a marketplace. The app
ships the marketplace too (`Resources/plugin-marketplace`, with the skill from
`Resources/skills`) and installs the plugin in each Claude Code account with `claude plugin`,
from a copy it keeps in `~/Library/Application Support/Codealong/plugin-marketplace`: the plugin
is the one the app answers, and it needs no network or access to the repo. The copy's version is
plugin.json's with a hash of the plugin's files after a `+`, and Claude Code updates a plugin
whenever that string changes, so at launch the app updates the plugin it installed whenever its
files, the skill included, changed. An account whose plugin comes from another marketplace (the
repo's, added by hand) keeps it until you pick Use Codealong's copy. A Claude Code account with
the plugin gets no skill copy, and one Codealong made there before is taken away, so sessions
don't see the skill twice. The permission hook hands the request to the app over its socket
and waits; the app holds it (`asks.rs`) until it's answered in one of Codealong's windows or on
another device, or answered or gone where the session runs, and the hook prints the decision,
or nothing. It prints nothing and exits 0 when the app isn't running, and an app older than the
hook fails its unknown subcommand the same quiet way, so Claude Code's own prompt is always
there to answer. A question (`AskUserQuestion`) is answered with the allow's `updatedInput`,
its questions with your answers beside them; a plan (`ExitPlanMode`) is left to the prompt,
since Claude Code drops a plain allow for it.

The command talks to the running app over `codealong.sock` next to the config (JSON lines, one
request and reply per line, mode 0600), and the app answers with the board its windows have and
applies todo changes through its engine, so nothing writes over anything. Without the app, todo
changes go to `todos.json` under an advisory lock the app takes too, and sessions come from a
light scan (no git, PRs or visitors).

## Permissions and safety

- The first time Codealong opens or focuses a terminal session, macOS asks whether it may
  control Ghostty, iTerm2 or Terminal (Automation). Without it, Codealong can still open plain
  Ghostty windows.
- A worktree or session folder on a removable volume, or in Documents, Desktop or Downloads,
  makes macOS ask whether Codealong may access it, and again after each update. Until someone
  answers, the board shows those worktrees without git and, after 10 seconds or so, names the
  folders it's waiting to open; everything else loads as usual.
- Codealong only reads: it never creates, resets or removes worktrees, never changes
  branches, and never types into a live session. Close idle terminals ends idle sessions'
  processes and closes their tabs, but only the ones you tick. Git runs with `GIT_OPTIONAL_LOCKS=0`.
- A session started In Codealong runs as a child of the app, through the agent's ACP adapter;
  what it's allowed to do is the agent's own permission setting for the account, and what it
  asks is answered only by you, in its panel. Its processes end when Codealong quits.
- Starting a session types the account's command into a new terminal window; the prompt goes in
  the window's environment (`CODEALONG_PROMPT`), so nothing in it is typed or needs quoting.
  Ghostty takes the folder and environment with the window; iTerm2 and Terminal get them from a
  short script in your temporary folder that sets them, deletes itself and becomes your login shell.
- Transcripts stay on the Mac; only titles, statuses and short excerpts are kept in memory.
  With Devices on, the devices you've linked get those too, and read a conversation or search
  what was said only by asking this device, directly. With remote control on, they can also send
  prompts, answer approvals and start sessions here; each such request is written to
  `~/Library/Logs/Codealong/devices.log`.

## Code

| Path | What |
| --- | --- |
| `src-tauri/src/engine.rs`, `throttle.rs` | Caches, file watching (FSEvents), git workers (spaced per worktree, results published in batches), GitHub, publishing the board |
| `src-tauri/src/board.rs` | The model's rules: status, bays, headlines, open targets (pure, tested) |
| `src-tauri/src/claude.rs`, `codex.rs` | Harness adapters: registries, transcripts, rollouts, incremental tailing |
| `src-tauri/src/opencode.rs`, `antigravity.rs` | Harness adapters: OpenCode's database and background service, Antigravity's conversation summaries |
| `src-tauri/src/acp.rs` | Sessions Codealong runs itself: an Agent Client Protocol adapter process per account, turns, permission requests, the streamed reply |
| `src-tauri/src/repos.rs`, `git.rs`, `github.rs` | Repo discovery, callsigns, worktrees, git status, PRs |
| `src-tauri/src/held.rs` | The repo scan's reads of folders it doesn't own, each on a thread of its own, so one macOS holds for a privacy prompt holds up only that folder |
| `src-tauri/src/launch.rs`, `launch/iterm.rs`, `launch/terminal_app.rs` | Terminal profiles: Ghostty, iTerm2 and Terminal through AppleScript (a session's tab found exactly, typing, closing), deep links |
| `src-tauri/src/editors.rs`, `src/editors.ts` | The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed, and the Open in menus |
| `src-tauri/src/flash.rs` | Outlines a focused terminal that came forward on another screen |
| `src-tauri/src/caret.rs`, `src/caret.ts` | A text cursor WebKit stopped drawing in a window you type in: typing into a page that says it isn't focused hands it its focus again, and the perf log says what macOS said of the window |
| `src-tauri/src/previews.rs`, `src-tauri/src/chrome.rs`, `src/ui/PreviewBits.tsx` | Previews: each worktree's URL and Chrome profile (`previews.json`), the servers listening in worktrees' folders (`procs::listeners`, the kernel's socket lists), their front pages read once, a kqueue for their exits; Chrome's profiles and opening a page in one; the row's chip, the panel's section and the URL sheet |
| `src-tauri/src/idle.rs`, `src/ui/CloseIdleDialog.tsx` | Close idle terminals: which sessions' tabs can close and why (pure, tested), and closing them |
| `src-tauri/src/start.rs`, `src/start.ts` | New sessions: where each account can start one, the launch it makes, the agent menu (every account on every model, and the effort levels) and Auto's pick |
| `src-tauri/src/quick.rs`, `src/quick.ts`, `src/ui/Quick.tsx` | The quick panel: its window (a panel that takes the keyboard over any app), its shortcut, and its page (what needs you, finding sessions, starting one) |
| `src-tauri/src/harness.rs` | The interface each harness implements (`Adapter`): a session's record, its conversation, what it waits on you to allow and answering it, renaming it (`src/ui/RenameDialog.tsx` in the main window); CLAUDE.md's "Adding a harness" says what else a new one needs |
| `src-tauri/src/transcript.rs`, `src/ui/SessionPanel.tsx`, `src/ui/SessionThread.tsx`, `src/ui/SessionReply.tsx` | A session's conversation, read a page at a time from its own record, the panel that shows it, and the box that sends it messages or answers what it waits on |
| `src-tauri/src/peek.rs`, `src/ui/Peek.tsx`, `src/ui/ReplyView.tsx` | The floating bar's peek: its window (a panel under the bar that takes the keyboard only on a click), where it opens for a pill and when it closes, and the session view it shares with the quick panel's ⇥ |
| `src-tauri/src/search.rs` | Searching what was said in sessions: an in-memory index of prompts and replies, read once per file and then from where it stopped |
| `src-tauri/src/route.rs`, `src-tauri/src/starts.rs`, `src/route.ts` | Suggestions in the quick panel: the request to Jev and reading its answer back, the key in the keychain, how your past starts weigh a new worktree, and which suggestions the panel shows |
| `src-tauri/src/todojev.rs` | Jev on todos: which todo a new session is for, whether a todo's work looks done, and a todo's suggested project, labels and dependencies |
| `src-tauri/src/send.rs` | Sending a prompt to a session that's already going, the way its harness takes one |
| `src-tauri/src/runcode.rs`, `src/ui/RunButton.tsx` | Run on a code block: the lines of a shell block to type into a new terminal (comments out, here-documents and quoted text whole), and the button |
| `src-tauri/src/peers.rs`, `peerfeed.rs` | Your other devices: links, connections, and the feed of worktrees and sessions sent a change at a time; `design/plans/devices.md` has the reasoning |
| `src-tauri/peer/` | codealong-peer, what goes between devices: the protocol (each side proves its device key), link codes and iroh, feeds, the requests and their answers, the worktree and session model, and a device that only watches. The iPhone app ([codealong-ios](https://github.com/codealong/codealong-ios)) builds on it; `design/plans/ios.md` has the plan |
| `src-tauri/src/tailnet.rs`, `devicekey.rs` | Reaching them on your tailnet through Tailscale's command (iroh is in codealong-peer), and this device's key in the keychain |
| `src/menus.ts`, `src/contextMenu.ts` | Right-click menus of sessions, worktrees, PRs and todos, each defined once and the same in every window (each says what it can do); native menus through Tauri |
| `src/devices.ts`, `src/ui/DevicesSection.tsx` | Other devices' worktrees in the views, and Settings › Devices |
| `src/ui/Sidebar.tsx`, `src/savedViews.ts` | The sidebar: the view and what narrows it, open or folded to a rail, and saved views |
| `src-tauri/src/cli.rs`, `server.rs`, `install.rs` | The `codealong` command (clap; JSON or tables; the app over its socket, else the files), the app's socket, and putting the skill and the command where agents find them |
| `src-tauri/src/todos.rs`, `src/todos.ts` | Todos: the store and its changes, linking sessions started from a todo; the list's sections, filters and prompt |
| `src-tauri/src/canvas.rs`, `src/canvas/`, `src/ui/CanvasView.tsx`, `src/ui/CanvasCard.tsx` | The Canvas view: its documents (what's on it, where; kept in sync with the board; Tidy's layouts), the WebGL layer (grid, links, zoomed-out blocks) and the HTML cards, notes and frames on top |
| `src-tauri/src/attachments.rs`, `src/attachInput.ts` | Files in descriptions and prompts: pasted, dropped or picked, kept by content, images shown in place, all handed to agents as paths |
| `src-tauri/src/usage.rs` | Plan usage: Claude through its sign-in and usage endpoint, Codex from its rollouts, none for OpenCode and Antigravity |
| `src-tauri/src/appserver.rs` | Codex actions through its app-server (archiving, renaming) |
| `src-tauri/src/claudeapp.rs` | The Claude app's session records (its IDs, what you archived there) |
| `src-tauri/src/config.rs`, `state.rs` | `config.toml` (accounts saved in place) and `state.json` (seen, pinned, archived, account folders already read) |
| `src-tauri/src/reusable.rs`, `src/reusable.ts` | Reusable worktrees: the marks made by hand (`worktrees.json`); `board::reusable`, `reuse_of`, `earlier` and `looks_reusable` have the rules, `git::new_work` reads the reflog, and `src/route.ts` sends Jev's new work to them |
| `src-tauri/src/prlinks.rs` | Sessions tied to PRs other than by branch: what their records show they did to a PR, and links made by hand (`pr-links.json`) |
| `src/prLinks.ts`, `src/ui/PrLink.tsx` | Links to PRs in conversations and prompts: the board's PRs by link, and the card a link shows on hover (`pr_card` for a PR the board doesn't list) |
| `src-tauri/src/chrometabs.rs` | The Chrome tabs a Claude Code session drove through Claude in Chrome, read from its transcript; which profile they're in (the extension's storage, then History); bringing its tab forward by AppleScript, or its last page again in that profile |
| `src-tauri/src/detect.rs` | The harness accounts on this device: folders, sign-ins, and the wrappers and aliases that run them |
| `src-tauri/src/bar.rs`, `barfeed.rs` | The bar's window (a panel over every app, floating where you put it or docked in the notch, found from the screen's sides of the menu bar, where it takes clicks only on the shape it shows; Window › Floating bar and Bar in the notch) and the small feed of what it shows, sent only when that changes |
| `src-tauri/src/origins.rs`, `src/lineage.ts`, `src-tauri/src/lineage.rs`, `src/ui/DesktopLines.tsx` | Where sessions came from: Codealong's own record of starts and forks (`origins.json`), the line drawn between them on the canvas and between desktop cards, and the clear window under the cards that draws it there |
| `src-tauri/src/desktop.rs`, `src/ui/DesktopPin.tsx` | Cards pinned to the desktop: a window each, made as the bar's is, where a new one goes, `desktop.json`, and what each shows, cut from the board and sent only when it changes; the page draws the Canvas view's card (`ui/CanvasCard.tsx`) |
| `src/` | The React UI, built from the design system's components and tokens; `lanes.ts` groups the board and mirrors the engine's status rules for the age filter; `pins.ts` keeps the pinned lists; `ahead.ts` shows sessions marked seen, archived, renamed or sent a message at once, in every window, until a board shows it; `prStatus.ts` and `ui/PrPills.tsx` say where a PR stands and which PRs are your move; `bar.ts`, `ui/Bar.tsx` and `ui/Notch.tsx` lay out the bar, floating or in the notch; `ui/Picker.tsx` is the quick panel's filterable menus; `ui/SettingsDialog.tsx` and `ui/AccountsSection.tsx` are Settings |
| `scripts/preview.sh` | Headless preview against real data, for screenshots and scripted checks |
| `design/tokens.json` | Design tokens; `npm run tokens` regenerates `src/styles/tokens.css` |

## License

Copyright 2026 Gordon L. Hempton. The code is under the [Apache License 2.0](LICENSE); copies
carry the [NOTICE](NOTICE). The Codealong name and logo are trademarks, and the license doesn't
grant rights to them: a fork should go by its own name.
