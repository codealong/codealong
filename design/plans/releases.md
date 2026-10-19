# Plan: an installer, releases and automatic updates

Status: phases 1 and 2 are built (2026-09-28). `scripts/release.sh` makes the installer, signed
with Gordon Hempton's Developer ID and notarized (Gatekeeper accepts the app and the disk image as
"Notarized Developer ID"), and each release's update; release builds find, install and offer it.
The routine (`scripts/version.sh`), CI's release build and the Homebrew tap are written
(2026-10-17). The repos are public since 2026-10-19, and 0.2.0 waits as a draft: see *Going
public* below. The decisions below are made.

## Going public

An audit on 2026-10-17 found no secrets, and the names from the owner's other work that were in
files and commit messages were rewritten out of both repos' histories that day, after which both
repos were recreated on GitHub so it keeps nothing of the old one (`.git/rewrite-2026-10-17`
holds the rules and the old-to-new commit map). Three commits of 2026-10-18 brought names back,
so the app repo's last 27 commits were rewritten again with the same rules on 2026-10-19 and the
repo recreated once more (`.git/rewrite-2026-10-19`); a pre-push hook on the owner's Mac now
refuses them. In this order:

1. Done 2026-10-19: `codealong/codealong`, `codealong/codealong-ios` and `codealong/homebrew-tap`
   are public, with private vulnerability reporting on the first two.
2. Done 2026-10-19: `scripts/release-secrets.sh` made the `release` and `homebrew` environments
   and their keys, from 1Password (environments need a paid plan on a private repo, so this
   waited for step 1).
3. Done 2026-10-18: `scripts/version.sh 0.2.0` tagged it, and with CI skipped while the repo was
   private, `scripts/release.sh --draft` built the draft on the owner's Mac. From the next
   version on, the tag has GitHub build the draft (release.yml), which waits for your approval in
   the run's page; about half an hour after that.
4. Install the draft's `Codealong.dmg` on a Mac or user account that has never had Codealong
   (*Check before the first public release* below), then publish the draft. Publishing moves
   `latest`, so installed copies find it and the website's button gets it, and homebrew.yml
   points the cask at it; then `brew install --cask codealong/tap/codealong` on a clean account.

## Decided: whose name is on it

Codealong is Gordon L. Hempton's, not a company's, and nothing in it names another company.
Releases are signed by his own Apple developer team, "Gordon Hempton" (team N3347S68VA), an
individual membership. `scripts/release.sh` signs with that team's Developer ID only
(`CODEALONG_TEAM_ID` changes it, for a fork) and refuses any other certificate in the keychain.

- **The certificate:** "Developer ID Application: Gordon Hempton (N3347S68VA)", made 2026-09-28
  from Apple's G2 intermediate, good until 2031-09-17. Its key was made on the Mac that builds
  releases and is in that Mac's login keychain; Apple can't send it again. The backup is in his
  1Password Private vault, "Codealong Developer ID signing key (Gordon Hempton, N3347S68VA)": the
  .p12 attached, opened by the item's password (which 1Password generated). That's how another
  Mac, or CI, signs.
- **Notarizing:** an App Store Connect API key, "Codealong notarization" (Developer role), made
  2026-09-28. The Mac that builds releases keeps it as notarytool's keychain profile `codealong`,
  so a release is `CODEALONG_NOTARY_PROFILE=codealong scripts/release.sh`. Apple lets the key
  file be downloaded once: it's in the same vault, "Codealong notarization key (App Store Connect
  API)", with its key ID and issuer ID, for another Mac (`xcrun notarytool store-credentials
  codealong --key AuthKey_<ID>.p8 --key-id <ID> --issuer <issuer>`) or CI.
- **The membership renews every year.** It had lapsed by 2026-09-28 and was renewed that day. A
  lapsed membership doesn't stop signed copies from running, but Apple won't notarize or issue
  certificates until it's renewed.
- **If Codealong becomes a company:** Apple converts an individual membership to an organization
  on request (Membership details, Convert to Organization; a D-U-N-S number, up to three weeks).
  The team ID, certificates and apps carry over and only the name changes, so installed copies
  keep their permissions. A new team instead would look like a different app to macOS, which
  would ask again for everything people allowed, controlling Ghostty first. (Keychain items
  don't care: Codealong reaches the keychain through `/usr/bin/security`.)
- The copyright is his too: `NOTICE`, the README's License section, and the app's About panel
  and Get Info (`bundle.copyright` in `tauri.conf.json`).

## Decided: releases come from codealong/codealong

`codealong/codealong` goes open source, and its GitHub Releases carry every installer and update
(decided 2026-09-28). The app asks
`https://github.com/codealong/codealong/releases/latest/download/latest.json`, the website's
Download button already points at `releases/latest`, and the Claude Code plugin's marketplace
reads the same repo. While the repo is still private those addresses answer 404 to everyone
else, so a copy that checks just finds no update; nothing breaks. Once it's public, building on
GitHub's own runners is free too (phase 3).

## Decided: the update key

Updates are signed with a minisign key pair made 2026-09-28 (`npx tauri signer generate`; key ID
6C22AE798C33F266). The public half is `plugins.updater.pubkey` in `tauri.conf.json`. The private
half and the password that opens it are only in 1Password, "Codealong update signing key" in the
Private vault, beside the other two; 1Password generated the password. Lose it and installed
copies can never update again (people reinstall by hand); leak it and whoever also controls the
download address can ship code to every copy. A release takes both straight from 1Password, with
nothing written to disk:

```bash
TAURI_SIGNING_PRIVATE_KEY="op://Private/Codealong update signing key/codealong-updater.key" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="op://Private/Codealong update signing key/password" \
CODEALONG_NOTARY_PROFILE=codealong op run -- scripts/release.sh --draft
```

`op` asks 1Password, which asks you to allow it (it times out if nobody does).

Decided against:

- **Handing out the installer before updates work.** Every copy without the updater has to be
  replaced by hand, so phase 2 lands before the first copy leaves this Mac.
- **A .pkg installer.** It could put the `codealong` command in `/usr/local/bin`, but it asks for
  an admin password, needs a second certificate (Developer ID Installer), and the updater
  replaces an app bundle either way. Settings › Agents already installs the command.
- **Tauri's own disk image.** It arranges the window by scripting Finder, which opens a Finder
  window mid-build, needs permission to control Finder, and is skipped on CI, which then ships a
  plain window. dmgbuild writes the layout into the image's `.DS_Store` directly.
- **Sparkle.** The Mac's standard updater, but Objective-C with its own appcast format, bridged
  from Rust; Tauri's updater does the same job (a signed archive, swapped in place) on this stack.
- **The Mac App Store.** Its sandbox rules out reading `~/.claude` and `~/.codex`, running git and
  gh, scripting Ghostty and the command-line tool: most of Codealong.

## Phase 1: the installer (built)

`scripts/release.sh` builds `Codealong_<version>_universal.dmg`:

- The app for Apple silicon and Intel in one bundle (`--target universal-apple-darwin`; `--arch
  arm64` for a quicker one-architecture build), with thin link-time optimization.
- Signed with the Developer ID under the hardened runtime, which notarizing requires.
  `src-tauri/Entitlements.plist` grants the one exception the app needs: sending Apple Events,
  which is how `launch.rs` drives Ghostty through `osascript`. Without it a signed build could no
  longer open or focus a terminal. The signature is stable across releases, so macOS keeps what
  people allowed (a dev build's ad-hoc signature, on a Mac without an Apple Development
  certificate, changes with every build, which is why it's asked again).
- The app notarized and stapled, then put in a disk image whose window shows the app, an arrow
  and the Applications folder, with "Drag Codealong into Applications" below
  (`src-tauri/dmg/settings.py`; `scripts/dmg-background.mjs` draws the background from the
  design tokens). The image is signed, notarized and stapled too, so it opens without a warning
  even offline.
- It checks, before anything goes to Apple, what Gatekeeper will: the signature verifies, it's a
  Developer ID with a secure timestamp and the hardened runtime, the entitlements are in, and the
  binary has both architectures. The copy inside the image is checked again with `codesign
  --strict`: hiding the ".app" extension there would add a Finder-info attribute that fails it,
  so the extension isn't hidden. After notarizing, `spctl` has to accept the app and the image.
- dmgbuild 1.6.7 or later: earlier versions write a bookmark to the background that Finder on
  macOS 26.2 and later can't resolve, and the window comes up blank (checked on 26.6). It needs
  Python 3.10, newer than macOS's own 3.9; the script finds Homebrew's. The window's size counts
  its title bar, so about 368 of its 400 points show the background.
- It builds in `codealong-release`, beside the shared build folder: a release compiles
  everything again for two architectures, and in the shared folder it would hold cargo's lock
  over every other session's build and throw away their cache. Output goes to
  `codealong-release/dist/<version>/`, with a `.sha256` beside the image.
- Tauri only builds and signs; the script notarizes, so a keychain profile works (Tauri takes
  credentials only from variables) and no password ever goes on a command line.

## Phase 2: automatic updates (built)

Tauri's updater plugin, `tauri-plugin-updater` 2.12: the newest that runs on Tauri 2.11 (2.13
needs 2.12, and Cargo would upgrade Tauri for it without asking, so it's pinned `~2.12`). The app
fetches a small JSON manifest, and when it names a newer version for `darwin-aarch64` or
`darwin-x86_64`, downloads that `.app.tar.gz`, checks its signature against the public key
compiled into the app, moves the running bundle aside and moves the new one into its place. It
asks for an administrator's password only when the folder isn't writable. The running app keeps
running from memory; the new version starts at the next launch.

The manifest is a release asset, `latest.json`, and the app asks
`https://github.com/codealong/codealong/releases/latest/download/latest.json`. GitHub's
`latest` is the newest published release that isn't a prerelease, so drafts and prereleases are
invisible to it: publishing the draft is what ships an update. `requireSignedVersion` is on from
the first release: the signature carries the version it was made for (the Tauri CLI in use,
2.11.5, records it), so a tampered manifest can't pair a new version number with an older,
validly signed archive.

In the app (`updates.rs`, `src/ui/Updates.tsx`):

- **When it checks.** Two minutes after launch, once the first scan is done, then every six
  hours by the clock (so a Mac that slept catches up), and whenever asked. It downloads and
  installs in the background, then tells the windows an update is ready, with the version and
  its notes. `[updates] check = false` in config.toml stops the automatic checks; asking still
  works.
- **Where you see it.** "Restart to update to <version>" above the foot of the sidebar once an
  update is in place; "Check for updates…" in the app menu under About (which becomes "Restart to
  update to <version>", and opens Settings › General to show how a check goes); and in Settings ›
  General the version, how updates stand (up to date and when, the version waiting and its
  notes, or why it failed) with Check now or Restart to update, and "Look for updates by itself".
  It never restarts on its own mid-work. Restarting loses nothing: the sessions run in Ghostty,
  Claude's service and the Codex app, not in Codealong.
- **Dev builds never update.** A build from `relaunch.sh` that found a release would replace
  itself with it. The release script builds with `CODEALONG_RELEASE=1`, read at compile time
  (`option_env!`), and only such builds check; Settings says a development build doesn't update.
- **Running from where it was downloaded.** Opened straight from the disk image or from
  Downloads, the app runs from a read-only place (the image, or the random folder macOS runs a
  quarantined app from, App Translocation) and can't replace itself. Before downloading, it
  checks for that and says to move Codealong to Applications. Not built yet: offering to move
  itself there and reopen at launch.
- **For trying an update before it's released,** `CODEALONG_UPDATE_URL` points a release build at
  another manifest (https, as release builds refuse anything else).
- Not built yet: `codealong update [--check]`, so an agent or a terminal can ask.

In `scripts/release.sh`, after stapling: the app goes into `Codealong_<version>_<arch>.app.tar.gz`
(`tar --no-mac-metadata --no-xattrs`: the updater unpacks every entry as a file), signed with
`npx tauri signer sign --app-version <version>`. The script checks the signature is by the key
the app trusts (their key IDs), and that the archive unpacks to an app of this version that passes
`codesign --verify --strict`. It writes `latest.json` with the release's platforms pointing at the
archive and the release notes (`--notes <file>`, else the version tag's message, else the commit
subjects since the tag before), and with `--draft` makes the draft release: the image (also as
`Codealong.dmg`, so the website can link straight to `releases/latest/download/Codealong.dmg`),
its checksum, the archive, its signature and `latest.json`. `--draft` refuses a version that's
already released, a version tagged on another commit, and a tag that isn't on GitHub yet (GitHub
would make one of its own at publishing and refuse the real one ever after).

`updates::tests::an_update_replaces_the_app` (ignored) installs a release's own update over a
copy of an app through the updater itself, in Tauri's mock runtime: a local server hands out
the release's `latest.json` and archive, and the key is `tauri.conf.json`'s.

## Versions and the routine

- One version for the app, the command and the Claude Code plugin, kept in `tauri.conf.json`,
  `Cargo.toml` (and its lockfile), `package.json` (and its lockfile),
  `plugin/.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json`. The release script
  refuses to build while they disagree (the app is 0.1.0 and the plugin 0.2.0 until the first
  release). 0.x while settings and features move: a minor version for features, a patch for
  fixes. The updater compares these as semver.
- `scripts/version.sh <x.y.z>` sets them all, commits "Release x.y.z" on top of origin/main and
  tags it `vx.y.z`, with the release notes as the tag's message; `--push` pushes both. It refuses
  a version that isn't newer than the app's, and a first release without `--notes <file>`.
- Release notes come from the commit subjects since the last tag, which already read as feature
  sentences, trimmed by hand (`--notes`). They go in the GitHub release and in `latest.json`,
  which is what the app shows beside Restart.
- Each release: `version.sh --push`, which has CI build the draft (or `release.sh --draft` on a
  Mac with the keys, after pushing the tag), then `scripts/check-release.sh <version>` and
  installing the draft's image on a Mac or user account that has never had Codealong, then
  publish the draft. The website needs nothing: its button follows `releases/latest`; the
  Homebrew cask follows it by itself (phase 4). The `/release` skill
  (`.claude/skills/release/SKILL.md`) walks an agent through all of it, stopping for the owner's
  word at the notes, the build's approval and publishing.

## Phase 3: building releases on CI (written, not run yet)

`.github/workflows/release.yml`, on `v*` tags, on GitHub's `macos-26` arm64 runners (the SDK the
app links against decides how macOS draws it, so it's the macOS it's built on by hand), runs the
same `scripts/release.sh --draft`: it imports the Developer ID certificate (a .p12 in a secret)
into a keychain of its own, writes the API key file from a secret, and passes
`TAURI_SIGNING_PRIVATE_KEY` and its password. The secrets live in a `release` environment that
only `v*` tags may deploy to and that waits for your approval, so neither a changed workflow on a
branch nor a tag an agent pushed signs anything unseen. `scripts/release-secrets.sh` makes the
environment and fills it from 1Password, with nothing written to disk but the .p12 for a moment.

Environments need a paid plan on a private repository (the organization's is free), so the job
is skipped while the repo is private, and releases are built on a Mac with the script until
then. On a public repo the runners are free. A universal release is likely 30 to 45 minutes
there. The first run will tell what a runner does differently from this Mac: the tests run
there first, and Gatekeeper's checks.

## Phase 4: more ways in

- **Homebrew (written).** `codealong/homebrew-tap` (private until the launch) has
  `Casks/codealong.rb`: `brew install --cask codealong/tap/codealong` installs the release's
  universal image (the URL and checksum), links the bundle's binary as the `codealong` command,
  says `auto_updates true` so `brew upgrade` leaves updating to the app, quits the app before
  uninstalling, and `--zap` removes its folders under ~/Library. `brew style` and `brew audit`
  pass. `.github/workflows/homebrew.yml` moves its version and checksum when a release is
  published (not a prerelease), with a deploy key that writes to the tap alone, in a `homebrew`
  environment of `v*` tags without approval (release-secrets.sh makes it). Not tried yet: an
  install from it, which needs a public release. Homebrew's own cask list comes later, once the
  repo has the stars and forks it asks for.
- **The Claude Code plugin.** `/plugin marketplace add codealong/codealong` reads the repo too,
  so it works for others once the repo is public.
- **A beta channel.** Prereleases (`v0.3.0-beta.1`) don't move `latest`, so they need a manifest
  of their own at a fixed address: a release tagged `beta` whose `latest.json` each beta replaces
  (`gh release upload beta latest.json --clobber`). `[updates] channel = "beta"` points the
  updater there (`updater_builder().endpoints(…)`).

## When a release is bad

The updater only moves forward, so a release can't be taken back from copies that installed
it. Turning it back into a draft (`gh release edit v0.3.0 --draft`) stops it spreading at once,
since `latest.json` then comes from the release before; the fix ships as the next patch. A
published release's files are never replaced: the signature and the notarization cover those
exact bytes, and a copy may be halfway through downloading them.

## Check before the first public release

- Install from the image on a Mac, or a fresh user account, that has never had Codealong: the
  first-open dialog names the developer and nothing else warns.
- The permission prompts a new install meets, each once and saying why: controlling Ghostty;
  finding devices on the local network, when Devices is on and iroh reaches another Mac over the
  LAN (the bundle has no `NSLocalNetworkUsageDescription` yet, so macOS would ask without a
  reason); and access to other apps' data, if the shortcut check reads Maccy's sandbox container.
- The installer window in dark mode: Finder may draw the icon names in white over the light
  background (checked in light mode only).
- The Intel half, on an Intel Mac (under Rosetta, `--version` from the image ran, 2026-09-28).
- An update installed by a running release on a real Mac: `an_update_replaces_the_app` put the
  first notarized release's own update in place of a copy of an app through the updater
  (2026-09-28), but not from inside the app. Afterwards the `codealong` command's link should
  still work (it points at the bundle's path, which the update keeps), and the skill copies
  should refresh (`install::refresh_installed` runs at launch).
- On a Mac that also builds Codealong: builds of the repo are Codealong Dev (relaunch.sh, its own
  id and light icon), the release is Codealong, and they share settings and todos, so the one
  opened last quits the other and the `codealong` command follows it (built 2026-09-28).
