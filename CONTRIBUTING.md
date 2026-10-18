# Contributing to Codealong

Thanks for wanting to help. Codealong is built mostly by coding agents working with its
maintainer, so changes made with an agent are welcome here, under the same rules as any other:
a person stands behind every pull request and can explain every line of it.

## Before you start

- **A small fix** (a typo, a clear bug with an obvious fix): open a pull request.
- **Anything bigger** (a feature, a new harness, a change to how something behaves or looks):
  open an issue first and agree on the approach with a maintainer before you build it. A pull
  request for a change nobody agreed to may be closed, however good it is.
- **A security problem:** report it privately, as [SECURITY.md](SECURITY.md) says, never in an
  issue.

The most welcome contributions are another harness (Gemini CLI, Copilot CLI, Amp, Pi, or
whichever one you run) and Windows support; the README's
[Agents and systems](README.md#agents-and-systems) says where each starts.

## Pull requests

Every change from outside the maintainers comes as a pull request against `main`. The template
asks for these, and a pull request without them waits until it has them:

- **What it changes and why**, and the issue it closes.
- **How you checked it.** `npm run typecheck`, `npm run lint`, `npm test` and `cargo test` (in
  `src-tauri`) pass, and the engine builds without warnings. [docs/development.md](docs/development.md)
  has the commands and the browser checks (`scripts/checks.sh`).
- **Screenshots of every UI change.** Before and after, in light and dark, and on Linux too
  when the change shows keys. `scripts/preview.sh` takes them in headless Chrome without running
  the app: `PREVIEW_SCHEME=dark` for dark, `PREVIEW_PLATFORM=linux` for Linux, and its header
  lists every view and the sample data it can show. The preview renders your own sessions and
  repos, so crop each screenshot to what changed and blur anything you don't want public. What
  the preview can't show (a native menu, a window opening, the notch) gets a screenshot or a
  short recording of the real app, or a sentence saying it wasn't checked.
- **Whether you used AI tools, and which** (the agent and model), in a sentence. They're welcome;
  saying so helps the review. Read and understand everything you submit before you open the pull
  request: one that reads as unreviewed agent output is closed.
- **One change per pull request,** with unrelated clean-ups left for another.

A pull request is merged once a maintainer has reviewed it and its checks pass. Maintainers'
own agents land work on `main` directly, as [CLAUDE.md](CLAUDE.md) describes; that path is
theirs alone.

## How the code is written

[CLAUDE.md](CLAUDE.md) (also `AGENTS.md`) is the guide every change follows, for people and
agents alike: the commands, the conventions, how a change is checked, and where facts about each
harness live. Your agent reads it on its own; read its *Conventions* yourself before a change
that adds something new. [docs/development.md](docs/development.md) has building, the checks
and a map of the code.

## Licensing

Codealong is under the [Apache License 2.0](LICENSE). What you contribute is under it too
(the license's section 5): there's no separate agreement to sign. Contribute only what you
have the right to: your own work, or code under a license that allows it, named in the pull
request. The Codealong name and logo aren't covered by the license ([NOTICE](NOTICE)).

## Conduct

Be kind and assume good faith. Maintainers may close, lock or hide what isn't, and block
accounts that keep at it.
