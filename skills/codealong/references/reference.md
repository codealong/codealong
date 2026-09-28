# codealong command reference

Generated from `codealong --help` by `scripts/skill-reference.sh`. Don't edit by hand.

## codealong

```
Codealong from the command line: the todo list, every agent session on this Mac, the
worktrees they run in, and the accounts they run on.

With the Codealong app running, answers come from it and todo changes go through it.
Without it, the files are read directly and sessions show without git status or PRs.

Output is JSON when stdout isn't a terminal (so always, from an agent) or with --json.
Times are epoch milliseconds. Ids take a unique prefix. A session is named by its key
(claude:claude2:<uuid>) or its id; `me` means the session running this command (see
whoami). A worktree is named by its path; `.` means the one the current folder is in.

Usage: codealong [OPTIONS] <COMMAND>

Commands:
  status     Whether the app is running, and where the data lives
  accounts   The configured accounts: harness, sign-in, and with --usage how much of
             each plan limit is used
  whoami     Which session runs this command, its worktree, and its open todos: the ones
             linked to the session, then the worktree's when no other live session works
             there (in a checkout several agents share, those are the others' work). Run
             it first
  todo       Todos: work meant for an agent. Each has a title, a description, labels, a
             status (todo, in-progress, done) and the worktrees and sessions that took
             it on. The app lists open ones under In progress, Ready and Waiting
  sessions   Sessions on the board: every agent, live or not, with its status and where
             it works. Lists by default; `find` searches
  session    One session in full: status, excerpt, folder, branch, PR, how to resume it,
             and where its own records are
  worktrees  Worktrees the board knows: each with its branch, PR, status and the agents
             in it
  pr         A PR and the sessions that worked on it, and linking a session to a PR by
             hand. A session is linked by itself when its records show it made, edited
             or commented on the PR (`gh pr create` and the like), or when it worked in
             a worktree on the PR's branch while the PR was open
  start      Start a session for a todo (or several, on one prompt) or on a prompt, the
             way the app's New session does, and link it to the todos
  config     Codealong's own settings (config.toml): show them as the app reads them,
             and check a file before or after editing it. Change settings by editing the
             file; the app watches it
  install    Put the command on PATH, the skill into each account's skills folder, and
             the adapters Codealong runs sessions through
  devices    Your other devices (Settings › Devices): this device, the devices it's
             linked with, your devices on your tailnet that could be, and devices asking
             to link. Linking is how devices see each other's sessions; needs the app
             running and Devices on
  help       Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## codealong status

```
Whether the app is running, and where the data lives

Usage: codealong status [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong accounts

```
The configured accounts: harness, sign-in, and with --usage how much of each plan limit
is used

Usage: codealong accounts [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --usage
          Read plan usage: from the app when it runs, else one call per Claude account.
          Don't loop it; the usage endpoint rate-limits

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong whoami

```
Which session runs this command, its worktree, and its open todos: the ones linked to
the session, then the worktree's when no other live session works there (in a checkout
several agents share, those are the others' work). Run it first

Usage: codealong whoami [OPTIONS]

Options:
      --context
          Plain text for a session-start hook, even when piped: a line or two when a
          session or todo is found, nothing otherwise

      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo

```
Todos: work meant for an agent. Each has a title, a description, labels, a status (todo,
in-progress, done) and the worktrees and sessions that took it on. The app lists open
ones under In progress, Ready and Waiting

Usage: codealong todo [OPTIONS] <COMMAND>

Commands:
  list    Todos that aren't done, in the app's order. --status all for every one
  show    One todo in full, with attachments as full paths
  add     A new todo at the top of the list. Prints it
  update  Change a todo's fields. Only the ones given change
  start   Mark a todo in progress, and link the session running this command and its
          worktree to it
  done    Mark a todo done, and link the session running this command to it: finishing a
          todo says you worked on it
  reopen  Put a done or in-progress todo back to todo
  wait    Mark a todo as waiting, on something outside the list: a person's answer, a
          release, a date. It stays under Waiting until `todo ready`, until it's
          started, or until it's done
  ready   Say a todo marked as waiting is ready. One that waits on todos that aren't
          done stays under Waiting until they are
  move    Move a todo in the list: to the top or bottom, or before or after another. The
          app shows todos in this order within each section
  link    Link a session or a worktree to a todo
  unlink  Unlink a session or a worktree from a todo
  delete  Delete a todo. The app can't undo this once its window's undo is gone, so ask
          first
  labels  The labels, or add and remove one
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo list

```
Todos that aren't done, in the app's order. --status all for every one

Usage: codealong todo list [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --status <STATUS>
          todo, in-progress, done, open (todo and in-progress; the default) or all
          
          [default: open]

      --project <PROJECT>
          Only todos of this project

      --table
          Tables, even when piped

      --label <LABEL>
          Only todos with this label (by name)

      --verbose
          Say on stderr whether the app or the files answered

      --worktree <WORKTREE>
          Only todos linked to this worktree (a path, or `.`)

      --mine
          Only todos linked to the session running this command, or to its worktree

      --ready
          Only todos under Ready: not started, not marked as waiting, and waiting on no
          todo that isn't done. Pick work from these

      --waiting
          Only todos under Waiting: marked as waiting, or not started and waiting on a
          todo that isn't done

  -h, --help
          Print help
```

## codealong todo show

```
One todo in full, with attachments as full paths

Usage: codealong todo show [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo add

```
A new todo at the top of the list. Prints it

Usage: codealong todo add [OPTIONS] <TITLE>

Arguments:
  <TITLE>
          One line

Options:
  -d, --description <DESCRIPTION>
          Plain text. Markdown reads fine in the app

      --json
          JSON output. The default when stdout isn't a terminal

      --description-file <FILE>
          Read the description from a file, or from stdin with `-`

      --table
          Tables, even when piped

      --project <PROJECT>
          The project it belongs to. Set from the worktree when one is linked

      --verbose
          Say on stderr whether the app or the files answered

  -l, --label <LABEL>
          A label by name, made if it doesn't exist. Repeatable

      --attach <FILE>
          A file to attach: copied into Codealong's store and referenced from the
          description. Repeatable

      --worktree <WORKTREE>
          Link a worktree: a path, or `.` for the current folder's

      --session <SESSION>
          Link a session: a key, an id, or `me`. A linked session is one working on it,
          so this marks it in progress; leave it out when noting work for later

      --depends-on <ID>
          A todo this one waits on: it's blocked until that one is done. Repeatable

      --wait-for <WHAT>
          Mark it as waiting from the start, on this: a person's answer, a release, a
          date

  -h, --help
          Print help
```

## codealong todo update

```
Change a todo's fields. Only the ones given change

Usage: codealong todo update [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --title <TITLE>
          

  -d, --description <DESCRIPTION>
          Replaces the description. Keep any attachment references you want to keep

      --table
          Tables, even when piped

      --description-file <FILE>
          Read the new description from a file, or stdin with `-`

      --verbose
          Say on stderr whether the app or the files answered

      --status <STATUS>
          todo, in-progress or done. in-progress links the session running this command
          and its worktree, as `todo start` does; done links the session, as `todo done`
          does

      --project <PROJECT>
          

      --add-label <ADD_LABEL>
          Add a label by name (made if new). Repeatable

      --remove-label <REMOVE_LABEL>
          Remove a label by name. Repeatable

      --add-dependency <ID>
          Wait on another todo too. Repeatable. Refused if that one already waits on
          this one

      --remove-dependency <ID>
          Stop waiting on a todo. Repeatable

  -h, --help
          Print help
```

## codealong todo start

```
Mark a todo in progress, and link the session running this command and its worktree to
it

Usage: codealong todo start [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo done

```
Mark a todo done, and link the session running this command to it: finishing a todo says
you worked on it

Usage: codealong todo done [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo reopen

```
Put a done or in-progress todo back to todo

Usage: codealong todo reopen [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo wait

```
Mark a todo as waiting, on something outside the list: a person's answer, a release, a
date. It stays under Waiting until `todo ready`, until it's started, or until it's done

Usage: codealong todo wait [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --for <WHAT>
          What it waits for, in a few words. Keeps the note it has when left out

      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo ready

```
Say a todo marked as waiting is ready. One that waits on todos that aren't done stays
under Waiting until they are

Usage: codealong todo ready [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo move

```
Move a todo in the list: to the top or bottom, or before or after another. The app shows
todos in this order within each section

Usage: codealong todo move [OPTIONS] <--top|--bottom|--before <ID>|--after <ID>> <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --top
          

      --bottom
          

      --table
          Tables, even when piped

      --before <ID>
          Just before this todo

      --verbose
          Say on stderr whether the app or the files answered

      --after <ID>
          Just after this todo

  -h, --help
          Print help
```

## codealong todo link

```
Link a session or a worktree to a todo

Usage: codealong todo link [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --session <SESSION>
          A session key, id, or `me`

      --table
          Tables, even when piped

      --worktree <WORKTREE>
          A worktree path, or `.`

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo unlink

```
Unlink a session or a worktree from a todo

Usage: codealong todo unlink [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --session <SESSION>
          A session key, id, or `me`

      --table
          Tables, even when piped

      --worktree <WORKTREE>
          A worktree path, or `.`

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo delete

```
Delete a todo. The app can't undo this once its window's undo is gone, so ask first

Usage: codealong todo delete [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo labels

```
The labels, or add and remove one

Usage: codealong todo labels [OPTIONS] [COMMAND]

Commands:
  add     Add a label. Color is one of gray, red, pink, purple, indigo, blue, teal,
          green; the next unused one when left out
  remove  Remove a label. Todos keep it until they change, so the app can undo
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo labels add

```
Add a label. Color is one of gray, red, pink, purple, indigo, blue, teal, green; the
next unused one when left out

Usage: codealong todo labels add [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --color <COLOR>
          

      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong todo labels remove

```
Remove a label. Todos keep it until they change, so the app can undo

Usage: codealong todo labels remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong sessions

```
Sessions on the board: every agent, live or not, with its status and where it works.
Lists by default; `find` searches

Usage: codealong sessions [OPTIONS] [COMMAND]

Commands:
  find        Sessions whose title, excerpt, account, worktree, branch or PR contains
              the words (all of them, any case), or whose prompts and replies do, for
              sessions active in the last `[search] days`
  close-idle  Close the Ghostty tabs of sessions that sat idle and want nothing from
              you: idle or unread, quiet for --after, nothing left running in the
              background, and a last reply that asks nothing (Jev's read with `[jev]
              sessions` on, else no question mark at its end). A tab closes only when
              every session in it qualifies. Its agent's process ends; the conversation
              stays and resumes as usual. Lists what it would close with --dry-run; run
              that first
  help        Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --account <ACCOUNT>
          Only this account (claude, claude2, codex, opencode, antigravity)

      --verbose
          Say on stderr whether the app or the files answered

      --status <STATUS>
          Only this status: needs (waiting on you), error, working, unread (its turn
          ended, not seen), idle, stopped

      --project <PROJECT>
          Only this project

      --worktree <WORKTREE>
          Only sessions in this worktree (a path, or `.`)

      --live
          Only sessions with a running process

      --since <SINCE>
          Active within this long: 90m, 24h, 3d, 7d (the default), or all
          
          [default: 7d]

      --archived
          Archived ones too

      --limit <LIMIT>
          At most this many, most recently prompted first
          
          [default: 50]

  -h, --help
          Print help
```

## codealong sessions find

```
Sessions whose title, excerpt, account, worktree, branch or PR contains the words (all
of them, any case), or whose prompts and replies do, for sessions active in the last
`[search] days`

Usage: codealong sessions find [OPTIONS] [QUERY]...

Arguments:
  [QUERY]...
          

Options:
      --account <ACCOUNT>
          Only this account (claude, claude2, codex, opencode, antigravity)

      --json
          JSON output. The default when stdout isn't a terminal

      --status <STATUS>
          Only this status: needs (waiting on you), error, working, unread (its turn
          ended, not seen), idle, stopped

      --table
          Tables, even when piped

      --project <PROJECT>
          Only this project

      --verbose
          Say on stderr whether the app or the files answered

      --worktree <WORKTREE>
          Only sessions in this worktree (a path, or `.`)

      --live
          Only sessions with a running process

      --since <SINCE>
          Active within this long: 90m, 24h, 3d, 7d (the default), or all
          
          [default: 7d]

      --archived
          Archived ones too

      --limit <LIMIT>
          At most this many, most recently prompted first
          
          [default: 50]

  -h, --help
          Print help
```

## codealong sessions close-idle

```
Close the Ghostty tabs of sessions that sat idle and want nothing from you: idle or
unread, quiet for --after, nothing left running in the background, and a last reply that
asks nothing (Jev's read with `[jev] sessions` on, else no question mark at its end). A
tab closes only when every session in it qualifies. Its agent's process ends; the
conversation stays and resumes as usual. Lists what it would close with --dry-run; run
that first

Usage: codealong sessions close-idle [OPTIONS]

Options:
      --after <AFTER>
          How long a session has been quiet: 90m, 24h, 3d, 1w
          
          [default: 24h]

      --json
          JSON output. The default when stdout isn't a terminal

      --dry-run
          Only say what would close and what stays open, and why

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong session

```
One session in full: status, excerpt, folder, branch, PR, how to resume it, and where
its own records are

Usage: codealong session [OPTIONS] <COMMAND>

Commands:
  show  One session in full
  help  Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong session show

```
One session in full

Usage: codealong session show [OPTIONS] <KEY>

Arguments:
  <KEY>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong worktrees

```
Worktrees the board knows: each with its branch, PR, status and the agents in it

Usage: codealong worktrees [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --project <PROJECT>
          

      --bay <BAY>
          Only this bay: needs, working, unread, review, idle, merged

      --table
          Tables, even when piped

      --all
          Hidden ones too (worktrees outside every project that need nothing)

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong pr

```
A PR and the sessions that worked on it, and linking a session to a PR by hand. A
session is linked by itself when its records show it made, edited or commented on the PR
(`gh pr create` and the like), or when it worked in a worktree on the PR's branch while
the PR was open

Usage: codealong pr [OPTIONS] <COMMAND>

Commands:
  show    One PR: its state, branches, the worktrees on its branch and the sessions that
          worked on it. Needs the app, which reads GitHub
  link    Link a session to a PR it worked on that Codealong can't see by itself: a PR
          opened another way (the website, an API tool), or one worked on from a
          worktree that isn't on its branch
  unlink  Take a session off a PR it's listed on but didn't work on. It stays off,
          however it matched, until linked again
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong pr show

```
One PR: its state, branches, the worktrees on its branch and the sessions that worked on
it. Needs the app, which reads GitHub

Usage: codealong pr show [OPTIONS] <PR>

Arguments:
  <PR>
          The PR: its URL, owner/name#number, or a number (#123) in the repo of the
          current folder's worktree

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong pr link

```
Link a session to a PR it worked on that Codealong can't see by itself: a PR opened
another way (the website, an API tool), or one worked on from a worktree that isn't on
its branch

Usage: codealong pr link [OPTIONS] <PR>

Arguments:
  <PR>
          The PR: its URL, owner/name#number, or a number (#123) in the repo of the
          current folder's worktree

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --session <SESSION>
          A session key, id, or `me`
          
          [default: me]

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong pr unlink

```
Take a session off a PR it's listed on but didn't work on. It stays off, however it
matched, until linked again

Usage: codealong pr unlink [OPTIONS] <PR>

Arguments:
  <PR>
          The PR: its URL, owner/name#number, or a number (#123) in the repo of the
          current folder's worktree

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --session <SESSION>
          A session key, id, or `me`
          
          [default: me]

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong start

```
Start a session for a todo (or several, on one prompt) or on a prompt, the way the app's
New session does, and link it to the todos

Usage: codealong start [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --todo <TODO>
          The todo to start: its title and description become the prompt, and the
          session is linked to it. Give it more than once to start several todos in one
          session, on one prompt that has each of them

      --prompt <PROMPT>
          The prompt, when not starting a todo

      --table
          Tables, even when piped

      --account <ACCOUNT>
          The account to run it on. `auto` picks as the quick panel does: a preferred
          account with room for the model (config.toml's `preferred`), and among those
          the one whose weekly limit resets soonest
          
          [default: auto]

      --verbose
          Say on stderr whether the app or the files answered

      --worktree <WORKTREE>
          The worktree to run in: a path, or `.` (the default)
          
          [default: .]

      --new-worktree
          Have Claude Code make a fresh worktree for it (`claude --worktree`)

      --in <PLACE>
          Where it runs. Without it, where the account's `start` setting says, else the
          terminal (the Codex app for Codex)

          Possible values:
          - terminal
          - background: Claude Code's background service, with no terminal until you
            attach one
          - claude-app
          - codex-app
          - codealong:  In Codealong itself, over the account's Agent Client Protocol
            adapter. Needs the app running

      --model <MODEL>
          A model for a terminal or background start: fable, opus, sonnet, haiku, or a
          full name

      --dry-run
          Print what the start would do (the command and environment, or the link)
          without starting anything

  -h, --help
          Print help (see a summary with '-h')
```

## codealong config

```
Codealong's own settings (config.toml): show them as the app reads them, and check a
file before or after editing it. Change settings by editing the file; the app watches it

Usage: codealong config [OPTIONS] <COMMAND>

Commands:
  show   The settings as the app reads them, the file's path and text, its error if it
         doesn't parse, the terminal and editor profiles, and accounts found on this Mac
         that aren't configured
  check  Whether a config.toml parses and passes the app's checks: the live one, or FILE
  help   Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong config show

```
The settings as the app reads them, the file's path and text, its error if it doesn't
parse, the terminal and editor profiles, and accounts found on this Mac that aren't
configured

Usage: codealong config show [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong config check

```
Whether a config.toml parses and passes the app's checks: the live one, or FILE

Usage: codealong config check [OPTIONS] [FILE]

Arguments:
  [FILE]
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong install

```
Put the command on PATH, the skill into each account's skills folder, and the adapters
Codealong runs sessions through

Usage: codealong install [OPTIONS]

Options:
      --command
          Only the command: a symlink on PATH to this binary

      --json
          JSON output. The default when stdout isn't a terminal

      --skill
          Only the skill, into the accounts' skills folders

      --table
          Tables, even when piped

      --acp
          Only the Agent Client Protocol adapters that let Codealong run Claude Code and
          Codex sessions itself (npm packages, in Codealong's folder)

      --verbose
          Say on stderr whether the app or the files answered

      --account <ACCOUNT>
          Only these accounts (by name). Every configured one by default

      --bin <DIR>
          Where the symlink goes. /usr/local/bin when writable, else ~/.local/bin

  -h, --help
          Print help
```

## codealong devices

```
Your other devices (Settings › Devices): this device, the devices it's linked with, your
devices on your tailnet that could be, and devices asking to link. Linking is how
devices see each other's sessions; needs the app running and Devices on

Usage: codealong devices [OPTIONS] [COMMAND]

Commands:
  link      Ask one of your devices on your tailnet to link, by name or short id. Prints
            the six digits both devices show; then say yes on that device (its app, or
            `codealong devices answer` there)
  answer    Say yes to a device asking to link, by name or short id, once it shows the
            same six digits (--no to say no)
  unlink    Unlink a device, here and on it
  code      A link code for a device that isn't on your tailnet; enter it there with
            `use-code`. It works once, for ten minutes
  use-code  Link with the device that made this link code
  help      Print this message or the help of the given subcommand(s)

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong devices link

```
Ask one of your devices on your tailnet to link, by name or short id. Prints the six
digits both devices show; then say yes on that device (its app, or `codealong devices
answer` there)

Usage: codealong devices link [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong devices answer

```
Say yes to a device asking to link, by name or short id, once it shows the same six
digits (--no to say no)

Usage: codealong devices answer [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --no
          

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong devices unlink

```
Unlink a device, here and on it

Usage: codealong devices unlink [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong devices code

```
A link code for a device that isn't on your tailnet; enter it there with `use-code`. It
works once, for ten minutes

Usage: codealong devices code [OPTIONS]

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

## codealong devices use-code

```
Link with the device that made this link code

Usage: codealong devices use-code [OPTIONS] <CODE>

Arguments:
  <CODE>
          

Options:
      --json
          JSON output. The default when stdout isn't a terminal

      --table
          Tables, even when piped

      --verbose
          Say on stderr whether the app or the files answered

  -h, --help
          Print help
```

