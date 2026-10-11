// A PR's changed files, its details' Changes tab (PrInspector.tsx `PrDetails`): a row each, its
// name first and its folder after it, quieter, opening to its diff. Once there are enough to look
// for one, a filter sits beside the heading and stays at the top while the list scrolls under
// it: what's typed narrows the list to the paths that hold every word (prFiles.ts), `-word`
// leaving some out, and lists after them the files whose changes mention it, so a name is found
// where the PR changed it as well as where it lives. The words are marked in the paths and in the
// diffs. ↑ ↓ pick a file, ⏎ opens or closes it, Escape clears the filter, then leaves it; while
// it filters, the files it lists open and close together. ⌘F goes to the filter where the
// window says so (`keys`), and so does a click on the file count over the details
// (`FilesFinder`), each showing the tab first (`onShow`). What's typed is kept for each PR while
// the window shows another.
//
// A diff has each line's numbers before and after (prFiles.ts `parsePatch`), one under the other
// (unified) or, where the list is wide enough, side by side (split), and its long lines wrap or
// scroll sideways; both are kept as you left them, for every PR. Where the window can open files
// (`FileOpener`), each row has, at its end, its diff in your editor (the editor's own icon; its
// side before is where the PR's branch began, its side after your copy in the worktree that
// holds the PR, else the PR's) and the file on GitHub, which show as the pointer comes onto the
// row; a right-click gives those and the file at its first change (menus.ts `prFileMenu`), and a
// line's number after opens the file at that line, in the worktree that holds the PR.

import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import { useLatest } from '../latest';
import { filterFiles, fileQuery, marks, parsePatch, pieces, splitPath, splitRows, type DiffLine, type FileHit } from '../prFiles';
import { flag, oneOf, useStored } from '../stored';
import type { PrFile } from '../types';
import { cx, Icon, Kbd, Seg } from './primitives';

/** How diffs are drawn, as you left them, for every PR. */
const LAYOUT_KEY = 'codealong.diffLayout';
const WRAP_KEY = 'codealong.diffWrap';
const LAYOUTS = ['unified', 'split'] as const;
type Layout = (typeof LAYOUTS)[number];
const LAYOUT_CODEC = oneOf(LAYOUTS);

/** The narrowest the list is offered side by side at: less leaves each side too little. */
const SPLIT_FROM = 760;

/** An editor a file opens in, as its button shows it: its name and its app's icon. */
export interface FileEditor {
  name: string;
  icon?: string;
}

/** How a window opens a PR's changed files outside it (PrInspector.tsx `PrDetails` builds it). */
export interface FileOpener {
  /** The editor a diff opens in, where one of yours can. */
  diff: FileEditor | null;
  /** Your editor, where a worktree on this Mac holds the PR: a file opens there at a line. */
  file: FileEditor | null;
  /** Where a diff's side after comes from, for its button's tooltip. */
  after: string;
  open: (file: PrFile, how: 'diff' | 'file', line?: number | null) => void;
  github: (file: PrFile) => void;
  /** Its right-click menu, with the line it opens at. */
  menu: (file: PrFile) => void;
}

/** The fewest files the filter is offered for: fewer read at a glance. */
const FILTER_FROM = 8;

/** The most a filter's files open at once. */
const OPEN_ALL = 30;

const PATCH_LINES = 400;

/** What's typed in each PR's filter, kept while the window goes from one PR to the next. */
const queries = new Map<string, string>();

/** Text with `words` marked where they fall. */
function Marked({ text, ranges, from, to }: { text: string; ranges: [number, number][]; from?: number; to?: number }) {
  return (
    <>
      {pieces(text, ranges, from, to).map((p, i) => (p.hit ? <mark key={i}>{p.text}</mark> : p.text))}
    </>
  );
}

/** How a diff is drawn: one under the other or side by side, and whether long lines wrap. */
interface View {
  layout: Layout;
  wrap: boolean;
}

const SIGN: Record<DiffLine['kind'], string> = { add: '+', del: '−', ctx: '', hunk: '', note: '' };

/** A file's diff, numbered (`parsePatch`). Opened for what its changes mention (`seek`), the first
 *  mention comes into view. With `onLine`, a line's number after opens the file at that line. */
function Patch({ text, words, seek, view, onLine }: { text: string; words: string[]; seek: boolean; view: View; onLine?: { title: (line: number) => string; run: (line: number) => void } }) {
  const [all, setAll] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (seek) box.current?.querySelector('mark')?.scrollIntoView({ block: 'center' });
    // Only as it opens: the words changing later move nothing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const lines = useMemo(() => parsePatch(text), [text]);
  // A file opened for what its changes mention shows them all, so none of it is below a fold.
  const mentions = words.length > 0 && lines.slice(PATCH_LINES).some((l) => marks(l.text, words).length > 0);
  const shown = all || mentions ? lines : lines.slice(0, PATCH_LINES);
  const code = (l: DiffLine) => (words.length > 0 ? <Marked text={l.text} ranges={marks(l.text, words)} /> : l.text);
  const num = (n: number | null, after: boolean) =>
    n !== null && after && onLine ? (
      <button type="button" className="ca-patch-num is-link" title={onLine.title(n)} onClick={() => onLine.run(n)}>
        {n}
      </button>
    ) : (
      <span className="ca-patch-num">{n}</span>
    );
  // A line across the whole diff: a hunk's header, or git's note about a newline.
  const across = (l: DiffLine, key: number) => (
    <div key={key} className={`ca-patch-row is-${l.kind}`}>
      <span className="ca-patch-across">{l.text}</span>
    </div>
  );
  const side = (l: DiffLine | null, after: boolean): ReactNode =>
    l ? (
      <>
        {num(after ? l.new : l.old, after)}
        <span className="ca-patch-sign">{SIGN[l.kind]}</span>
        <span className="ca-patch-code">{code(l)}</span>
      </>
    ) : (
      <span className="ca-patch-none" />
    );
  const split = view.layout === 'split';
  return (
    <>
      <div className={cx('ca-patch', split ? 'is-split' : 'is-unified', (split || view.wrap) && 'is-wrap')} ref={box}>
        {split
          ? splitRows(shown).map((r, i) =>
              r.kind === 'pair' ? (
                <div key={i} className={cx('ca-patch-row', r.left === r.right ? 'is-ctx' : 'is-change')}>
                  <span className={cx('ca-patch-half', r.left && `is-${r.left.kind}`)}>{side(r.left, false)}</span>
                  <span className={cx('ca-patch-half', r.right && `is-${r.right.kind}`)}>{side(r.right, true)}</span>
                </div>
              ) : (
                across(r.line, i)
              ),
            )
          : shown.map((l, i) =>
              l.kind === 'hunk' || l.kind === 'note' ? (
                across(l, i)
              ) : (
                <div key={i} className={`ca-patch-row is-${l.kind}`}>
                  {num(l.old, false)}
                  {num(l.new, true)}
                  <span className="ca-patch-sign">{SIGN[l.kind]}</span>
                  <span className="ca-patch-code">{code(l)}</span>
                </div>
              ),
            )}
      </div>
      {shown.length < lines.length && (
        <button type="button" className="ca-more-btn" onClick={() => setAll(true)}>
          Show all {lines.length.toLocaleString('en-US')} lines
        </button>
      )}
    </>
  );
}

const STATUS_LETTER: Record<string, string> = { added: 'A', removed: 'D', modified: 'M', renamed: 'R', copied: 'C', changed: 'M' };
const STATUS_WORD: Record<string, string> = { added: 'Added', removed: 'Deleted', modified: 'Changed', renamed: 'Renamed', copied: 'Copied', changed: 'Changed' };

/** An editor's own icon, else a line icon in its place. */
function EditorMark({ editor, icon }: { editor: FileEditor; icon: string }) {
  return editor.icon ? <img className="ca-appicon" src={editor.icon} alt="" /> : <Icon name={icon} size={14} />;
}

function FileRow({ hit, open, active, words, view, opener, onToggle }: { hit: FileHit; open: boolean; active: boolean; words: string[]; view: View; opener?: FileOpener; onToggle: () => void }) {
  const { file, inPath, lines } = hit;
  const { dir, name } = splitPath(file.path);
  const row = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (active) row.current?.scrollIntoView({ block: 'nearest' });
  }, [active]);
  const editor = opener?.file;
  const onLine = opener && editor ? { title: (n: number) => `Open in ${editor.name} at line ${n}`, run: (n: number) => opener.open(file, 'file', n) } : undefined;
  const diffTitle = opener?.diff && `Open the diff in ${opener.diff.name}: where its branch began, beside ${opener.after}`;
  return (
    <div ref={row} className={cx('ca-prfile', open && 'is-open', active && 'is-active')}>
      <div
        className="ca-prfile-bar"
        onContextMenu={
          opener &&
          ((e) => {
            e.preventDefault();
            opener.menu(file);
          })
        }
      >
        <button type="button" className="ca-prfile-head" aria-expanded={open} onClick={onToggle} title={file.previous ? `${file.previous} → ${file.path}` : file.path}>
          <Icon name="chevron-right" size={12} stroke={2} />
          <span className={`ca-prfile-status is-${file.status}`} title={STATUS_WORD[file.status]}>
            {STATUS_LETTER[file.status] ?? '·'}
          </span>
          <span className="ca-prfile-path">
            <span className="ca-prfile-name">
              <Marked text={file.path} ranges={inPath} from={file.path.length - name.length} />
            </span>
            {dir && (
              <span className="ca-prfile-dir">
                <bdi>
                  <Marked text={file.path} ranges={inPath} to={dir.length} />
                </bdi>
              </span>
            )}
          </span>
          {lines > 0 && <span className="ca-prfile-mentions">{lines === 1 ? '1 line' : `${lines} lines`}</span>}
          <span className="ca-diff">
            +{file.additions} −{file.deletions}
          </span>
        </button>
        {opener && (
          <span className="ca-prfile-acts">
            {opener.diff && (
              <button type="button" className="ca-prfile-act" aria-label={`Open diff in ${opener.diff.name}`} title={diffTitle || undefined} onClick={() => opener.open(file, 'diff')}>
                <EditorMark editor={opener.diff} icon="git-compare" />
              </button>
            )}
            <button type="button" className="ca-prfile-act" aria-label="Open on GitHub" title="Open on GitHub" onClick={() => opener.github(file)}>
              <Icon name="external" size={13} />
            </button>
          </span>
        )}
      </div>
      {open &&
        (file.patch ? (
          <Patch text={file.patch} words={words} seek={lines > 0} view={view} onLine={onLine} />
        ) : (
          <p className="ca-empty ca-prfile-nodiff">
            GitHub doesn’t show a diff for this file here.
            {opener?.diff && (
              <button type="button" className="ca-linkbtn" onClick={() => opener.open(file, 'diff')}>
                Open diff in {opener.diff.name}
              </button>
            )}
            {opener && (
              <button type="button" className="ca-linkbtn" onClick={() => opener.github(file)}>
                Open on GitHub
              </button>
            )}
          </p>
        ))}
    </div>
  );
}

/** The element `el` scrolls in. */
function scroller(el: Element | null): Element | null {
  for (let p = el?.parentElement; p; p = p.parentElement) {
    if (/(auto|scroll)/.test(getComputedStyle(p).overflowY)) return p;
  }
  return null;
}

/** Takes the cursor to the filter, from outside the list. */
export interface FilesFinder {
  find: () => void;
}

interface PrFilesProps {
  /** The PR's key, for what's typed in its filter. */
  prKey: string;
  files: PrFile[];
  /** How many files the PR changes, of which `files` may be the first. */
  total: number;
  more: boolean;
  /** ⌘F goes to the filter. */
  keys?: boolean;
  /** Shows the tab the files are on, before the filter's gone to. */
  onShow?: () => void;
  /** Where the window can open the files outside it. */
  opener?: FileOpener;
}

/** Whether `el` is at least `px` wide, as it's drawn and as it changes. It's measured again as it
 *  draws, too: a list drawn while its tab was hidden is nothing wide until the tab shows, and
 *  WebKit holds back a resize's word while a window is hidden. */
function useWide(el: { readonly current: Element | null }, px: number): boolean {
  const [wide, setWide] = useState(false);
  // On every draw: the same answer again draws nothing more, so it can't go round.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useLayoutEffect(() => {
    const at = el.current;
    if (at) setWide(at.getBoundingClientRect().width >= px);
  });
  useLayoutEffect(() => {
    const at = el.current;
    if (!at) return;
    const watch = new ResizeObserver(([entry]) => setWide(entry.contentRect.width >= px));
    watch.observe(at);
    return () => watch.disconnect();
  }, [el, px]);
  return wide;
}

export const PrFiles = forwardRef<FilesFinder, PrFilesProps>(function PrFiles({ prKey, files, total, more, keys = false, onShow, opener }, ref) {
  const [query, setQuery] = useState(() => queries.get(prKey) ?? '');
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  const [active, setActive] = useState(-1);
  const [focused, setFocused] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const top = useRef<HTMLDivElement>(null);
  const filterable = files.length >= FILTER_FROM;
  const [layout, setLayout] = useStored<Layout>(LAYOUT_KEY, LAYOUT_CODEC, 'unified');
  const [wrap, setWrap] = useStored(WRAP_KEY, flag, false);

  useEffect(() => {
    queries.set(prKey, query);
  }, [prKey, query]);

  const words = useMemo(() => fileQuery(query).want, [query]);
  const filtering = query.trim().length > 0;
  const { named, mentioned } = useMemo(() => filterFiles(files, query), [files, query]);
  const rows = useMemo(() => [...named, ...mentioned], [named, mentioned]);

  const section = useRef<HTMLElement>(null);
  // Side by side only where there's room for both sides; your choice is kept for when there is.
  const wide = useWide(section, SPLIT_FROM);
  const view: View = { layout: wide ? layout : 'unified', wrap };
  const find = () => {
    // Shown at once, so what follows finds the list where it's drawn.
    if (onShow) flushSync(onShow);
    // The list comes into view at its heading, unless the heading's in view already: stuck at
    // the top over the files, it stays where you'd scrolled to.
    const at = top.current?.getBoundingClientRect();
    const view = scroller(section.current)?.getBoundingClientRect();
    if (at && view && (at.top < view.top || at.bottom > view.bottom)) section.current?.scrollIntoView({ block: 'start' });
    input.current?.focus();
    input.current?.select();
  };
  useImperativeHandle(ref, () => ({ find }));

  const finding = useLatest(filterable ? find : null);
  useEffect(() => {
    if (!keys) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || !finding.current) return;
      if (e.key.toLowerCase() !== 'f' || !e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
      e.preventDefault();
      finding.current();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [keys, finding]);

  const toggle = (path: string) =>
    setOpen((was) => {
      const next = new Set(was);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  const allOpen = rows.length > 0 && rows.every((h) => open.has(h.file.path));
  const openAll = () =>
    setOpen((was) => {
      const next = new Set(was);
      for (const h of rows) {
        if (allOpen) next.delete(h.file.path);
        else next.add(h.file.path);
      }
      return next;
    });

  const type = (text: string) => {
    setQuery(text);
    setActive(text.trim() ? 0 : -1);
  };

  // What the filter found is counted against the files it looked through, which can be fewer than
  // the PR's (the first hundred) or, read later than the board, more.
  const count = filtering ? `${rows.length} of ${files.length}` : `${total}${more ? ` (first ${files.length} shown)` : ''}`;
  const row = (h: FileHit, i: number) => <FileRow key={h.file.path} hit={h} open={open.has(h.file.path)} active={focused && i === active} words={words} view={view} opener={opener} onToggle={() => toggle(h.file.path)} />;

  return (
    <section className="ca-insp-sec ca-prfiles-sec" aria-label="Files changed" ref={section}>
      <div className={cx('ca-prfiles-top', filterable && 'is-filterable')} ref={top}>
        <h4 className="ca-label">Files changed · {count}</h4>
        {filtering && rows.length > 1 && rows.length <= OPEN_ALL && (
          <button type="button" className="ca-linkbtn" onClick={openAll}>
            {allOpen ? 'Close all' : 'Open all'}
          </button>
        )}
        <span className="ca-prfiles-view">
          {wide && (
            <Seg<Layout>
              look="icon"
              label="How diffs are drawn"
              value={layout}
              onPick={setLayout}
              options={[
                { id: 'unified', label: 'Unified', icon: 'layout-rows', title: 'Unified: what it removed over what it added' },
                { id: 'split', label: 'Split', icon: 'layout-columns', title: 'Split: before on the left, after on the right' },
              ]}
            />
          )}
          {view.layout === 'unified' && (
            <Seg<'wrap'> look="icon" toggles label="Long lines" value={wrap ? 'wrap' : null} onPick={() => setWrap((w) => !w)} options={[{ id: 'wrap', label: 'Wrap long lines', icon: 'text-wrap', title: wrap ? 'Long lines wrap. Click to scroll them sideways' : 'Long lines scroll sideways. Click to wrap them' }]} />
          )}
        </span>
        {filterable && (
          <label className="ca-prfiles-filter" title="Words a file’s path holds, any order; -word leaves out the paths that hold it. Files whose changes mention them come after.">
            <Icon name="search" size={12} />
            <input
              ref={input}
              value={query}
              placeholder="Filter files"
              aria-label="Filter the files changed"
              spellCheck={false}
              onChange={(e) => type(e.target.value)}
              onFocus={() => setFocused(true)}
              onBlur={() => setFocused(false)}
              onKeyDown={(e) => {
                if (e.nativeEvent.isComposing || e.metaKey || e.ctrlKey || e.altKey) return;
                if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                  e.preventDefault();
                  const step = e.key === 'ArrowDown' ? 1 : -1;
                  setActive((at) => Math.max(0, Math.min(rows.length - 1, at + step)));
                } else if (e.key === 'Enter') {
                  const h = rows[Math.max(0, active)];
                  if (!h) return;
                  e.preventDefault();
                  setActive(Math.max(0, active));
                  toggle(h.file.path);
                } else if (e.key === 'Escape') {
                  // Clears the filter, then leaves it; the window's own Escape waits for the next.
                  e.preventDefault();
                  if (query) type('');
                  else input.current?.blur();
                }
              }}
            />
            {query ? (
              <button
                type="button"
                className="ca-prfiles-clear"
                aria-label="Clear the filter"
                title="Clear the filter"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => {
                  type('');
                  input.current?.focus();
                }}
              >
                <Icon name="x" size={11} />
              </button>
            ) : (
              keys && !focused && <Kbd keys={['⌘', 'F']} />
            )}
          </label>
        )}
      </div>
      <div className="ca-prfiles">{named.map((h, i) => row(h, i))}</div>
      {mentioned.length > 0 && (
        <>
          <h5 className="ca-prfiles-sub">Mentioned in the changes</h5>
          <div className="ca-prfiles">{mentioned.map((h, i) => row(h, named.length + i))}</div>
        </>
      )}
      {filtering && rows.length === 0 && (
        <p className="ca-empty">
          No file’s path or changes hold that.{' '}
          <button type="button" className="ca-linkbtn" onClick={() => type('')}>
            Clear the filter
          </button>
        </p>
      )}
    </section>
  );
});
