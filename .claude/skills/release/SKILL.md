---
name: release
description: Ship a new version of the Codealong Mac app. Picks the version, writes the release notes, tags it so GitHub builds, signs and notarizes it, checks the draft as a download gets it, and publishes it once the user says so; installed copies, the website's Download and the Homebrew cask then follow by themselves. Use when the user says they're ready to release, wants a new version out, or runs /release (optionally with a version, /release 0.4.0).
---

# Release

A release is a version tag on `main`. `scripts/version.sh` makes it: it sets the one version the
app, the `codealong` command and the Claude Code plugin share, commits "Release x.y.z" and tags it
with the release notes as the tag's message. Pushing the tag has GitHub build the release
(`.github/workflows/release.yml`): universal, signed with the Developer ID, notarized, with the
signed update and `latest.json`, as a draft release. Publishing the draft is what makes it live:
installed copies find it (their updater reads `releases/latest/download/latest.json`), the
website's Download buttons follow `releases/latest`, and `.github/workflows/homebrew.yml` points
the cask in `codealong/homebrew-tap` at it. `design/plans/releases.md` has the decisions behind
all of it.

Everything a release does is public and can't be taken back once people have it. The user says
go at four points, in chat, and you never go past one without it:

1. the version and the notes, before the tag is pushed;
2. the build, which waits for their approval on GitHub (the `release` environment's reviewer);
3. publishing the draft;
4. deleting or moving any tag or release.

## 1. Where things stand

```bash
git fetch -q origin
git describe --tags --abbrev=0 origin/main          # the last version tag
gh release list -R codealong/codealong --limit 5     # drafts waiting, the latest published
gh run list -R codealong/codealong --workflow linux.yml --limit 1   # main's last Linux check
```

Say what's there: the last release and whether it's published, any draft still waiting (a draft
older than `main` is stale: offer to delete it, with its tag, once the new one is out), and
whether `main` is green. Something already wrong on `main` is fixed and landed first, the usual
way (CLAUDE.md's *Landing work*), not in the release.

## 2. The version and the notes

The version is `x.y.z`: while it's 0.x, a minor version when there are features, a patch when
there are only fixes; `x.y.z-beta.n` for a prerelease, which GitHub marks as one, so publishing it
moves neither `latest` (installed copies) nor the cask. It must be newer than the app's
(`src-tauri/tauri.conf.json`), and a version is never used twice. Take the one the user gave, or
propose one.

Write the notes from what changed since the last tag:

```bash
git log --no-merges --format='%h %s' <last tag>..origin/main
gh release view <last tag> -R codealong/codealong --json body --jq .body   # the last notes, for their shape
```

Commit subjects here are long feature sentences; the notes are for people using the app, not
for the repo. Leave out "Release …" commits, "After Codex's review" follow-ups (they're part of
the feature before them), docs, tests, previews and refactors nobody can see. Then, in a file in
your scratchpad:

- a first line that says what this version is about, in a sentence;
- **What's new**: one bullet per feature, a short phrase first and then what it does, plain words
  and sentence case, the biggest first;
- **Fixes**, when there are any worth naming;
- for the first public release only, an **Install** section: the disk image from the release
  page, or `brew install --cask codealong/tap/codealong`.

The notes go on the release page and in `latest.json`, which the app shows beside Restart when it
offers the update, so keep them to what a user would care about. They're public: no names from
the owner's other work or this Mac's real projects, people or sessions (the pre-push hook checks
commits, not the tag's message, so this is on you).

Show the user the version and the notes, and change them until they say go.

## 3. Tag it

From a worktree of its own at `origin/main`, never the main checkout, which other sessions'
uncommitted edits may be in (`version.sh` refuses to run anywhere HEAD isn't `origin/main` or
something's uncommitted):

```bash
git worktree add --detach ../codealong-release-<version> origin/main
cd ../codealong-release-<version>
scripts/version.sh <version> --notes <notes file> --push
```

It commits "Release <version>", tags `v<version>`, and pushes the commit to `main` and then the
tag. A push turned away because `main` moved: nothing reached GitHub but what it says it pushed;
delete the local tag (`git tag -d v<version>`), remove the worktree, and do this step again from
a fresh one. Once the tag is pushed, from the main checkout:

```bash
scripts/catch-up-main.sh
git worktree remove ../codealong-release-<version>
```

## 4. The build

```bash
gh run list -R codealong/codealong --workflow release.yml --limit 1 --json databaseId,url,status
```

The run waits for the user's approval before it starts ("Review deployments" on its page). Give
them the run's link and ask them to approve it there. Approve it yourself only when they tell you
to in chat, as them, through
`gh api repos/codealong/codealong/actions/runs/<id>/pending_deployments`
(POST `environment_ids[]` from a GET of the same, `state=approved`, a comment).

Once approved it takes about half an hour, most of it Apple's notarization. Wait for it in the
background (`gh run watch <id> -R codealong/codealong --exit-status`). When it fails:

- `gh run view <id> -R codealong/codealong --log-failed` says where.
- A step that timed out or Apple's service turning a request away is worth one rerun:
  `gh run rerun <id> -R codealong/codealong --failed`.
- A fault in the build or the code is fixed and landed the usual way; the tag then names a
  commit without the fix, so the release goes out as the next version (a tag is never moved
  without the user's word).
- With GitHub's runners down, the same release builds on a Mac with the keys:
  `scripts/release.sh --draft` from the tagged commit, with the keys from 1Password
  (`design/plans/releases.md`).

## 5. Check the draft

```bash
scripts/check-release.sh <version>
```

It downloads the draft as GitHub serves it, marks it as downloaded the way a browser does, and
checks every file, Gatekeeper and notarization, the signature and entitlements, the version, the
license files, and the update and `latest.json`. Every line must be ok.

What it can't check is a person opening it: the first-open prompt, the setup wizard on a first
run, the permission prompts. Offer the user the disk image to try before publishing
(`gh release download v<version> -R codealong/codealong -p Codealong.dmg -D <scratchpad>`). On a
Mac that also runs Codealong Dev, opening the release quits Dev: they share their settings, and
the last one opened wins.

## 6. Publish, when the user says so

```bash
gh release edit v<version> -R codealong/codealong --draft=false --latest
```

A prerelease is published with `--prerelease` and without `--latest`. Then check that it went
everywhere it should:

```bash
curl -sL https://github.com/codealong/codealong/releases/latest/download/latest.json | jq -r .version   # installed copies
curl -sIL -o /dev/null -w '%{http_code}\n' https://github.com/codealong/codealong/releases/latest/download/Codealong.dmg   # the website's Download: 200
gh run list -R codealong/codealong --workflow homebrew.yml --limit 1   # moved the cask
gh api repos/codealong/homebrew-tap/contents/Casks/codealong.rb --jq .content | base64 -d | grep -E '^  (version|sha256)'
```

The cask's `version` is the new one and its `sha256` the disk image's (`Codealong_<version>_universal.dmg.sha256`).
A Homebrew run that skipped or failed leaves the cask on the last version: its log says why, and
`gh workflow run homebrew.yml -R codealong/codealong --ref v<version>` runs it again (on the
tag: it takes the release from the ref it runs on, and its environment only lets `v*` tags in).

## 7. Say how it went

The release page's link, what `check-release.sh` and the checks after publishing showed, and what
nobody checked yet (a person's first open, an installed copy taking the update). Then offer to
delete a stale draft and its tag, if step 1 found one.
