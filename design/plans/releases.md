# Plan: an installer, releases and automatic updates

Status: the installer is built (`scripts/release.sh`, 2026-09-28), signed with Gordon Hempton's
Developer ID and notarized: Gatekeeper accepts the app and the disk image as "Notarized Developer
ID", both architectures run from the image, and the window draws as designed in light mode.
Everything after it is a plan. The decisions come first, because each is baked into the first
copy anyone installs.

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

## Decide before the first release

1. **Where downloads and updates come from.** `codealong/codealong` is private, and the website's
   Download button already points at its `releases/latest`, which only works once the repo is
   public. The code is now under the Apache License 2.0, which suggests that's the plan: then
   releases go on the repo's GitHub Releases. If the source has to stay private for a while, a
   public `codealong/releases` repo can carry the binaries alone. The update address is compiled into
   every copy, so it should be one we keep: a GitHub URL is fine for years (codealong.com is
   parked at Sedo, not ours).
2. **Who keeps the update key.** `npx tauri signer generate -w ~/.tauri/codealong.key` makes the
   key pair updates are signed with. The public half goes in `tauri.conf.json`; the private half
   and its password go in 1Password beside the other two and, later, CI's secrets. Lose it and
   installed copies can never update again (people reinstall by hand); leak it and whoever also
   controls the download address can ship code to every copy.

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
  people allowed (the dev builds' ad-hoc signature changes with every build, which is why they
  ask again).
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

## Phase 2: automatic updates

Tauri's updater plugin (`tauri-plugin-updater` 2.13, read from its source): the app fetches a
small JSON manifest, and when it names a newer version for `darwin-aarch64` or
`darwin-x86_64`, downloads that `.app.tar.gz`, checks its signature against the public key
compiled into the app, moves the running bundle aside and moves the new one into its place. It
asks for an administrator's password only when the folder isn't writable. The running app keeps
running from memory; the new version starts at the next launch.

The manifest is a release asset, `latest.json`, and the app asks
`https://github.com/codealong/codealong/releases/latest/download/latest.json`. GitHub's
`latest` is the newest published release that isn't a prerelease, so drafts and prereleases are
invisible to it: publishing the draft is what ships an update. `requireSignedVersion` goes on
from the first release: the signature then carries the version it was made for (the Tauri CLI
in use, 2.11.5, records it), so a tampered manifest can't pair a new version number with an
older, validly signed archive.

In the app:

- **When it checks.** Two minutes after launch, once the first scan is done, then every six
  hours, and whenever asked. It downloads and installs in the background, then tells the windows
  an update is ready, with the version and its notes. `[updates] check = false` in config.toml
  stops the automatic checks; asking still works.
- **Where you see it.** A quiet "Restart to update" button at the bottom of the sidebar once an
  update is installed; "Check for updates…" in the app menu under About; a version line in
  Settings › General ("Version 0.2.0 · Up to date", or the version waiting and a Restart
  button); and `codealong update [--check]`, so an agent or a terminal can ask. It never
  restarts on its own mid-work. Restarting loses nothing: the sessions run in
  Ghostty, Claude's service and the Codex app, not in Codealong.
- **Dev builds never update.** A build from `relaunch.sh` that found a release would replace
  itself with it. The release script builds with `CODEALONG_RELEASE=1`, read at compile time
  (`option_env!`), and only such builds check.
- **Moving to Applications.** Opened straight from the disk image or from Downloads, the app
  runs from a read-only place (the image, or the random folder macOS runs a quarantined app from,
  App Translocation) and can't replace itself, and the `codealong` command's link would point
  into the image. At launch, a bundle under `/Volumes/` or `/AppTranslocation/` offers to move
  itself to Applications and reopen from there.

In `scripts/release.sh`: after stapling, `tar` the app into
`Codealong_<version>_universal.app.tar.gz` and sign it with `npx tauri signer sign
--app-version <version>` (the private key from `TAURI_SIGNING_PRIVATE_KEY`); write `latest.json`
with both `darwin-aarch64` and `darwin-x86_64` pointing at that one universal archive; and make a
draft release, `gh release create v<version> --draft`, with the image (also as `Codealong.dmg`,
so the website can link straight to `releases/latest/download/Codealong.dmg`), its checksum, the
archive, its signature and `latest.json`.

## Versions and the routine

- One version for the app, the command and the Claude Code plugin, kept in `tauri.conf.json`,
  `Cargo.toml`, `package.json`, `plugin/.claude-plugin/plugin.json` and
  `.claude-plugin/marketplace.json` (all 0.1.0 today; the release script refuses to build when
  the first three disagree). 0.x while settings and features move: a minor version for features,
  a patch for fixes. The updater compares these as semver.
- `scripts/version.sh <x.y.z>` (to write) sets them all, commits "Release x.y.z" and tags
  `vx.y.z`.
- Release notes come from the commit subjects since the last tag, which already read as feature
  sentences, trimmed by hand. They go in the GitHub release and in `latest.json`, which is what
  the app shows beside Restart.
- Each release: `version.sh`, then `release.sh` (phase 3: CI does it on the tag), then install
  the draft's image on a Mac or user account that has never had Codealong, then publish the
  draft. The website needs nothing: its button follows `releases/latest`.

## Phase 3: building releases on CI

A workflow on `v*` tags, on GitHub's arm64 macOS runners, runs the same `scripts/release.sh`: it
imports the Developer ID certificate (a .p12 in a secret) into a temporary keychain, writes the
API key file from a secret, and passes `TAURI_SIGNING_PRIVATE_KEY` and its password. The
secrets live in a `release` environment that needs your approval, so a changed workflow on a
branch can't read them. On a public repo the runners are free; on a private one macOS minutes
count ten times against the free plan's 2,000 a month, so about 200, and a universal release is
likely 20 to 30 minutes. Until the repo is public, releases are built on a Mac with the script.

## Phase 4: more ways in

- **Homebrew.** A cask in a `codealong/homebrew-tap` repo (`brew install --cask
  codealong/tap/codealong`): the image's URL and checksum, `auto_updates true` so `brew upgrade`
  leaves updating to the app, `depends_on macos: ">= :ventura"`, and a `zap` stanza for
  `~/Library/Application Support/Codealong`. The release script can bump it. Homebrew's own
  cask list comes later, once the repo has the stars and forks it asks for.
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
- The Intel half, on an Intel Mac or under Rosetta (`arch -x86_64
  /Applications/Codealong.app/Contents/MacOS/codealong --version`).
- An update, end to end, from a local manifest before the first real one: the `codealong`
  command's link still works afterwards (it points at the bundle's path, which the update keeps),
  and the skill copies refresh (`install::refresh_installed` runs at launch).
- On a Mac that also builds Codealong: the release in Applications and a dev build share the
  socket, config and shortcut, so only one should run at a time.
