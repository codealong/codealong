import { describe, expect, it } from 'vitest';
import { diffRuns, fileOnGitHub, fileQuery, filterFiles, firstChanged, marks, parsePatch, pieces, splitPath, splitRows } from './prFiles';
import type { PrFile } from './types';

const file = (path: string, patch: string | null = null): PrFile => ({ path, status: 'modified', additions: 1, deletions: 0, patch });

const FILES = [
  file('apps/app/src/common/NamesCountBadge.tsx', '@@ -0,0 +1,2 @@\n+export function NamesCountBadge() {}\n+// counts names'),
  file('apps/app/src/common/entity-select/TeamMultiSelect.tsx', '@@ -1 +1 @@\n-old\n+const team = createProject();'),
  file('apps/app/src/common/entity-select/userOrTeamValues.spec.ts', '@@ -0,0 +1 @@\n+it("splits teams")'),
  file('apps/api/src/project/createProject.ts', '@@ -3 +3 @@\n-function createProject(a)\n+function createProject(a, teams)'),
  file('yarn.lock'),
];
const paths = (hits: { file: PrFile }[]) => hits.map((h) => h.file.path);

describe('fileQuery', () => {
  it('takes words to find and words after a dash to leave out, in lower case', () => {
    expect(fileQuery('  Team  -SPEC select ')).toEqual({ want: ['team', 'select'], skip: ['spec'] });
    // A dash alone is looked for, as in a path.
    expect(fileQuery('- x')).toEqual({ want: ['-', 'x'], skip: [] });
    expect(fileQuery('')).toEqual({ want: [], skip: [] });
  });
});

describe('filterFiles', () => {
  it('lists every file with nothing typed', () => {
    const { named, mentioned } = filterFiles(FILES, ' ');
    expect(paths(named)).toEqual(FILES.map((f) => f.path));
    expect(named.every((h) => h.inPath.length === 0)).toBe(true);
    expect(mentioned).toEqual([]);
  });

  it('lists the paths that hold every word, in any case and order', () => {
    expect(paths(filterFiles(FILES, 'select team').named)).toEqual(['apps/app/src/common/entity-select/TeamMultiSelect.tsx', 'apps/app/src/common/entity-select/userOrTeamValues.spec.ts']);
    expect(paths(filterFiles(FILES, 'BADGE').named)).toEqual(['apps/app/src/common/NamesCountBadge.tsx']);
  });

  it('leaves out the paths that hold a word after a dash', () => {
    expect(paths(filterFiles(FILES, 'team -spec').named)).toEqual(['apps/app/src/common/entity-select/TeamMultiSelect.tsx']);
    expect(paths(filterFiles(FILES, '-apps').named)).toEqual(['yarn.lock']);
  });

  it('lists the files whose changes mention every word after the ones named', () => {
    const { named, mentioned } = filterFiles(FILES, 'createproject');
    expect(paths(named)).toEqual(['apps/api/src/project/createProject.ts']);
    expect(mentioned.map((h) => [h.file.path, h.lines])).toEqual([['apps/app/src/common/entity-select/TeamMultiSelect.tsx', 1]]);
    // Every word, though not on one line; a file without a diff mentions nothing.
    expect(filterFiles(FILES, 'names counts').mentioned.map((h) => [h.file.path, h.lines])).toEqual([['apps/app/src/common/NamesCountBadge.tsx', 2]]);
    expect(filterFiles(FILES, 'lock').mentioned).toEqual([]);
    // What leaves a path out leaves its changes out too.
    expect(filterFiles(FILES, 'teams -project').mentioned.map((h) => h.file.path)).toEqual(['apps/app/src/common/entity-select/userOrTeamValues.spec.ts']);
  });

  it('marks where the words fall in a path', () => {
    const [hit] = filterFiles(FILES, 'team multi').named;
    expect(hit.inPath).toEqual([[34, 43]]);
    expect(hit.file.path.slice(34, 43)).toBe('TeamMulti');
  });
});

describe('marks and pieces', () => {
  it('merges stretches that touch or overlap', () => {
    expect(marks('aXbXab', ['ab', 'x'])).toEqual([[1, 2], [3, 6]]);
    expect(marks('teamteam', ['team', 'amte'])).toEqual([[0, 8]]);
    expect(marks('abc', [''])).toEqual([]);
  });

  it('cuts a stretch of the text into marked and plain pieces', () => {
    expect(pieces('src/team/a.ts', [[4, 8]])).toEqual([{ text: 'src/', hit: false }, { text: 'team', hit: true }, { text: '/a.ts', hit: false }]);
    // Only the part asked for, the marks cut to it.
    expect(pieces('src/team/a.ts', [[4, 8]], 6)).toEqual([{ text: 'am', hit: true }, { text: '/a.ts', hit: false }]);
    expect(pieces('abc', [])).toEqual([{ text: 'abc', hit: false }]);
  });

  it('says a path as its name and folder', () => {
    expect(splitPath('a/b/c.ts')).toEqual({ dir: 'a/b', name: 'c.ts' });
    expect(splitPath('README.md')).toEqual({ dir: '', name: 'README.md' });
  });
});

describe('parsePatch', () => {
  const patch = '@@ -3,4 +3,5 @@ function a() {\n   one\n-  two\n+  Two\n+  three\n   four\n\\ No newline at end of file\n@@ -20 +21,2 @@\n-x\n+y\n+z\n';

  it('numbers each line on the sides it is on, from its hunk', () => {
    expect(parsePatch(patch).map((l) => [l.kind, l.old, l.new, l.text])).toEqual([
      ['hunk', null, null, '@@ -3,4 +3,5 @@ function a() {'],
      ['ctx', 3, 3, '  one'],
      ['del', 4, null, '  two'],
      ['add', null, 4, '  Two'],
      ['add', null, 5, '  three'],
      ['ctx', 5, 6, '  four'],
      ['note', null, null, 'No newline at end of file'],
      ['hunk', null, null, '@@ -20 +21,2 @@'],
      ['del', 20, null, 'x'],
      ['add', null, 21, 'y'],
      ['add', null, 22, 'z'],
    ]);
  });

  it('pairs each run of removed lines with the added ones after it, side by side', () => {
    const said = (p: string) => splitRows(parsePatch(p)).map((r) => (r.kind === 'pair' ? `${r.left?.text ?? '·'}|${r.right?.text ?? '·'}` : r.kind));
    expect(said(patch)).toEqual(['hunk', '  one|  one', '  two|  Two', '·|  three', '  four|  four', 'note', 'hunk', 'x|y', '·|z']);
    // Removed lines after added ones aren't put beside them.
    expect(said('@@ -1,2 +1,2 @@\n+a\n-b\n')).toEqual(['hunk', '·|a', 'b|·']);
  });

  it('colors each hunk a side at a time, a line kept on both', () => {
    const sides: string[] = [];
    // Each line's runs say which side colored it and its place there.
    const color = (code: string) => {
      sides.push(code);
      const n = sides.length;
      return code.split('\n').map((_, i) => [{ text: `${n}.${i}`, tone: null }]);
    };
    const lines = parsePatch(patch);
    const { before, after } = diffRuns(lines, color);
    expect(sides).toEqual(['  one\n  two\n  four', '  one\n  Two\n  three\n  four', 'x', 'y\nz']);
    expect(lines.map((l) => before.get(l)?.[0].text ?? '·')).toEqual(['·', '1.0', '1.1', '·', '·', '1.2', '·', '·', '3.0', '·', '·']);
    expect(lines.map((l) => after.get(l)?.[0].text ?? '·')).toEqual(['·', '2.0', '·', '2.1', '2.2', '2.3', '·', '·', '·', '4.0', '4.1']);
    // A side that can't be colored leaves its lines plain.
    const plain = diffRuns(lines, () => null);
    expect(plain.before.size + plain.after.size).toBe(0);
  });

  it('names a file on GitHub by the SHA-256 of its path', async () => {
    // sha256("README.md"), as `printf README.md | shasum -a 256` gives it.
    expect(await fileOnGitHub('https://github.com/a/b/pull/7', 'README.md')).toBe('https://github.com/a/b/pull/7/files#diff-b335630551682c19a781afebcf4d07bf978fb1f8ac04c6bf87428ed5106870f5');
  });

  it('opens a file at its first added line, else where its first hunk starts', () => {
    expect(firstChanged(parsePatch(patch))).toBe(4);
    expect(firstChanged(parsePatch('@@ -10,3 +10,2 @@\n a\n-b\n c'))).toBe(10);
    expect(firstChanged([])).toBeNull();
  });
});
