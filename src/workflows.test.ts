// What GitHub's workflows (.github/workflows) must hold to. It's here rather than beside them so
// that typecheck and lint read it, as land.test.ts is.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { readdirSync, readFileSync } from 'node:fs';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

const folder: string = fileURLToPath(new URL('../.github/workflows/', import.meta.url));
const workflows: { name: string; text: string }[] = (readdirSync(folder) as string[])
  .filter((name) => /\.ya?ml$/.test(name))
  .map((name) => ({ name, text: readFileSync(folder + name, 'utf8') as string }));

const indentOf = (line: string) => line.search(/\S/);

// Each step of a workflow that uses `action`, as its lines: from the step's `- ` (an `if:` may
// come before its `uses:`) to the next line as indented as that, or less.
function stepsUsing(text: string, action: string): string[] {
  const lines = text.split('\n');
  const steps: string[] = [];
  lines.forEach((line, i) => {
    if (!new RegExp(`^\\s*(- )?uses: ${action}@`).test(line)) return;
    let start = i;
    while (start > 0 && !/^\s*- /.test(lines[start])) start--;
    const indent = indentOf(lines[start]);
    let end = start + 1;
    while (end < lines.length && !(lines[end].trim() && indentOf(lines[end]) <= indent)) end++;
    steps.push(lines.slice(start, end).join('\n'));
  });
  return steps;
}

// setup-zig keeps its cache in .zig-cache inside the checkout, which git doesn't ignore: a rerun
// of v0.31.0's release build restored the first attempt's cache there, and release.sh, which
// builds only a clean checkout, turned it down. Its cache also takes a new entry every run
// (ci.yml says what that did to the Rust caches).
test("every workflow's setup-zig step leaves its cache off", () => {
  const steps = workflows.flatMap(({ name, text }) => stepsUsing(text, 'mlugg/setup-zig').map((step) => ({ name, step })));
  expect(new Set(steps.map((s) => s.name))).toEqual(new Set(['ci.yml', 'checks.yml', 'release.yml']));
  expect(steps.filter((s) => !/^\s+use-cache: false$/m.test(s.step))).toEqual([]);
});
