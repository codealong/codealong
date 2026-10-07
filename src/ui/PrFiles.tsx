// A PR's changed files, at the foot of its details (PrInspector.tsx `PrDetails`): a row each, its
// name first and its folder after it, quieter, opening to its diff. Once there are enough to look
// for one, a filter sits beside the heading and stays at the top while the list scrolls under
// it: what's typed narrows the list to the paths that hold every word (prFiles.ts), `-word`
// leaving some out, and lists after them the files whose changes mention it, so a name is found
// where the PR changed it as well as where it lives. The words are marked in the paths and in the
// diffs. ↑ ↓ pick a file, ⏎ opens or closes it, Escape clears the filter, then leaves it; while
// it filters, the files it lists open and close together. ⌘F goes to the filter where the
// window says so (`keys`), and so does a click on the file count over the details
// (`FilesFinder`). What's typed is kept for each PR while the window shows another.

import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useLatest } from '../latest';
import { filterFiles, fileQuery, marks, pieces, splitPath, type FileHit } from '../prFiles';
import type { PrFile } from '../types';
import { cx, Icon, Kbd } from './primitives';

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

/** A file's diff. Opened for what its changes mention (`seek`), the first mention comes into view. */
function Patch({ text, words, seek }: { text: string; words: string[]; seek: boolean }) {
  const [all, setAll] = useState(false);
  const pre = useRef<HTMLPreElement>(null);
  useLayoutEffect(() => {
    if (seek) pre.current?.querySelector('mark')?.scrollIntoView({ block: 'center' });
    // Only as it opens: the words changing later move nothing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const lines = text.split('\n');
  // A file opened for what its changes mention shows them all, so none of it is below a fold.
  const mentions = words.length > 0 && lines.slice(PATCH_LINES).some((l) => marks(l, words).length > 0);
  const shown = all || mentions ? lines : lines.slice(0, PATCH_LINES);
  return (
    <>
      <pre className="ca-patch" ref={pre}>
        {shown.map((l, i) => (
          <span key={i} className={l.startsWith('@@') ? 'is-hunk' : l.startsWith('+') ? 'is-add' : l.startsWith('-') ? 'is-del' : undefined}>
            {words.length > 0 ? <Marked text={l} ranges={marks(l, words)} /> : l || ' '}
            {'\n'}
          </span>
        ))}
      </pre>
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

function FileRow({ hit, open, active, words, onToggle }: { hit: FileHit; open: boolean; active: boolean; words: string[]; onToggle: () => void }) {
  const { file, inPath, lines } = hit;
  const { dir, name } = splitPath(file.path);
  const row = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (active) row.current?.scrollIntoView({ block: 'nearest' });
  }, [active]);
  return (
    <div ref={row} className={cx('ca-prfile', open && 'is-open', active && 'is-active')}>
      <button type="button" className="ca-prfile-head" aria-expanded={open} onClick={onToggle} title={file.path}>
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
      {open && (file.patch ? <Patch text={file.patch} words={words} seek={lines > 0} /> : <p className="ca-empty">GitHub doesn't show a diff for this file here. Open it on GitHub.</p>)}
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
}

export const PrFiles = forwardRef<FilesFinder, PrFilesProps>(function PrFiles({ prKey, files, total, more, keys = false }, ref) {
  const [query, setQuery] = useState(() => queries.get(prKey) ?? '');
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  const [active, setActive] = useState(-1);
  const [focused, setFocused] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const top = useRef<HTMLDivElement>(null);
  const filterable = files.length >= FILTER_FROM;

  useEffect(() => {
    queries.set(prKey, query);
  }, [prKey, query]);

  const words = useMemo(() => fileQuery(query).want, [query]);
  const filtering = query.trim().length > 0;
  const { named, mentioned } = useMemo(() => filterFiles(files, query), [files, query]);
  const rows = useMemo(() => [...named, ...mentioned], [named, mentioned]);

  const section = useRef<HTMLElement>(null);
  const find = () => {
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
  const row = (h: FileHit, i: number) => <FileRow key={h.file.path} hit={h} open={open.has(h.file.path)} active={focused && i === active} words={words} onToggle={() => toggle(h.file.path)} />;

  return (
    <section className="ca-insp-sec ca-prfiles-sec" aria-label="Files changed" ref={section}>
      <div className={cx('ca-prfiles-top', filterable && 'is-filterable')} ref={top}>
        <h4 className="ca-label">Files changed · {count}</h4>
        {filtering && rows.length > 1 && rows.length <= OPEN_ALL && (
          <button type="button" className="ca-linkbtn" onClick={openAll}>
            {allOpen ? 'Close all' : 'Open all'}
          </button>
        )}
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
