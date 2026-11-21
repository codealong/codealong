// Checks on the repo's notes and release-bundle files that used to live in Rust tests. Keeping
// them here lets edits to CLAUDE.md, AGENTS.md, LICENSE, NOTICE, THIRD_PARTY_NOTICES.md and
// .claude/rules/* run under `ui` in seconds instead of rebuilding the engine on macOS and Linux.
// @ts-expect-error no @types/node
import { existsSync, readdirSync, readFileSync } from 'node:fs';
// @ts-expect-error no @types/node
import { resolve } from 'node:path';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repo: string = fileURLToPath(new URL('..', import.meta.url));

describe('the notes every session reads and the files the bundle copies', () => {
  it('keeps CLAUDE.md within Codex’s 32 KiB limit and lists every .claude/rules/ file', () => {
    // CLAUDE.md is loaded into every session in this repo, and Codex reads only its first 32 KiB
    // (AGENTS.md is a link to it, cut at `project_doc_max_bytes`). What one area needs goes beside
    // its code, or in that area's file in .claude/rules/, which loads when a session reads a file
    // its `paths` name, and which CLAUDE.md lists.
    const notes: string = readFileSync(resolve(repo, 'CLAUDE.md'), 'utf8');
    const bytes = new TextEncoder().encode(notes).length;
    expect(bytes, `CLAUDE.md is ${bytes} bytes, over the 32 KiB Codex reads: move what only one area needs into its .claude/rules/ file`).toBeLessThanOrEqual(32 * 1024);
    expect(notes, "CLAUDE.md doesn't say where the snapshot modes are listed").toContain('`.claude/rules/snapshot.md`');
    const rulesDir = resolve(repo, '.claude/rules');
    const entries: string[] = readdirSync(rulesDir).filter((name: string) => name.endsWith('.md'));
    expect(entries.length, `no rules files in ${rulesDir}`).toBeGreaterThan(0);
    for (const name of entries) {
      expect(notes, `CLAUDE.md's *Where facts live* doesn't list .claude/rules/${name}`).toContain(`\`${name}\``);
      const text: string = readFileSync(resolve(rulesDir, name), 'utf8');
      expect(text.startsWith('---\npaths:\n'), `.claude/rules/${name} doesn't start with the \`paths\` it loads for`).toBe(true);
    }
  });

  it('has every file tauri.conf.json lists under bundle.resources and THIRD_PARTY_NOTICES.md', () => {
    const conf = JSON.parse(readFileSync(resolve(repo, 'src-tauri/tauri.conf.json'), 'utf8')) as { bundle?: { resources?: Record<string, string> } };
    const resources = Object.keys(conf.bundle?.resources ?? {});
    expect(resources.length).toBeGreaterThan(0);
    for (const rel of resources) {
      const target = resolve(repo, 'src-tauri', rel);
      expect(existsSync(target), `tauri.conf.json resource ${rel} is missing at ${target}`).toBe(true);
    }
    expect(existsSync(resolve(repo, 'THIRD_PARTY_NOTICES.md')), 'THIRD_PARTY_NOTICES.md is missing (scripts/licenses.sh bundles it into every release)').toBe(true);
  });
});
