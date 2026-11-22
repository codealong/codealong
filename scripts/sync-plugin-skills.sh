#!/usr/bin/env bash
# Keeps plugin/skills/ and plugin/.claude-plugin/icon.png as real copies of skills/codealong/
# and src-tauri/icons/icon.png, rather than symlinks (which Claude's plugin directory rejects).
# A cargo test in src-tauri/src/skills.rs fails if any copy drifts or becomes a symlink.
set -euo pipefail
cd "$(dirname "$0")/.."

rm -rf plugin/skills/codealong
mkdir -p plugin/skills/codealong
cp -R skills/codealong/. plugin/skills/codealong/

for name in orchestrate visualize meetings coordinate; do
  rm -f "plugin/skills/$name/SKILL.md"
  mkdir -p "plugin/skills/$name"
  cp "skills/codealong/references/$name.md" "plugin/skills/$name/SKILL.md"
done

cp src-tauri/icons/icon.png plugin/.claude-plugin/icon.png
echo "Synced plugin/skills/ and plugin/.claude-plugin/icon.png"
