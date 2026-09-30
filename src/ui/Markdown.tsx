// Just enough Markdown for what agents write back: headings, lists, quotes, fenced code, tables,
// and **bold**, *italics*, `code` and [links](url) inside lines. Built as React elements, never as
// HTML, so nothing in a transcript can put markup on the page. Web links, written as links or bare,
// open in the browser; any other link shows as its text.

import { Fragment, memo, type ReactNode } from 'react';
import { openUrl } from '../api';
import { CodeBlock } from './CopyButton';

const WEB = /^https?:\/\//i;
/** A bare web link in text, without the punctuation that ends a sentence around it. */
const BARE = /https?:\/\/[^\s<>()`]+[^\s<>()`.,;:!?'"*_]/g;

/** A web link that opens in the browser, never in the app's own window. */
function Link({ url, children }: { url: string; children: ReactNode }) {
  return (
    <a
      className="ca-md-a"
      href={url}
      title={url}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        void openUrl(url);
      }}
    >
      {children}
    </a>
  );
}

/** Plain text with its bare web links made links. */
function bare(text: string, key: number): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(BARE)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push(
      <Link key={`${key}-${m.index}`} url={m[0]}>
        {m[0]}
      </Link>,
    );
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'para'; text: string }
  /** `start`: an ordered list's first number, which goes on after a code block in the list. */
  | { kind: 'list'; ordered: boolean; items: string[]; start?: number }
  | { kind: 'quote'; text: string }
  /** `shell`: a finished block of shell commands, which can run in a new terminal. */
  | { kind: 'code'; text: string; shell: boolean }
  | { kind: 'table'; align: Align[]; head: string[]; rows: string[][] }
  | { kind: 'rule' };

type Align = 'left' | 'center' | 'right' | null;

const ITEM = /^\s*(?:[-*+]|(\d+)[.)])\s+(.*)$/;
/** A fence's languages for shell commands. A console session isn't one: it holds prompts and
 *  output, and a block with no language is as often output or a path as a command. */
const SHELLS = new Set(['bash', 'sh', 'zsh', 'fish', 'ksh', 'shell']);
/** A table's delimiter row, under its header: `| --- | :-: |`. */
const DELIM = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

/** A table row's cells: split on pipes that aren't escaped or inside `code`. */
function cells(line: string): string[] {
  const out: string[] = [];
  let cell = '';
  let tick = false;
  const s = line.trim().replace(/^\|/, '').replace(/(?<!\\)\|$/, '');
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === '\\' && s[i + 1] === '|') {
      cell += '|';
      i++;
    } else if (c === '`') {
      tick = !tick;
      cell += c;
    } else if (c === '|' && !tick) {
      out.push(cell.trim());
      cell = '';
    } else cell += c;
  }
  out.push(cell.trim());
  return out;
}

/** Whether a table starts at line i: a row with a pipe, then a delimiter row with as many cells. */
const tableAt = (lines: string[], i: number) =>
  lines[i].includes('|') && i + 1 < lines.length && DELIM.test(lines[i + 1]) && cells(lines[i]).length === cells(lines[i + 1]).length;

export function blocks(src: string): Block[] {
  const out: Block[] = [];
  const lines = src.replace(/\r\n?/g, '\n').split('\n');
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    if (/^\s*```/.test(line)) {
      const lang = line.trim().slice(3).trim().split(/\s+/)[0].toLowerCase();
      // A fence in a list is indented with the list, and so are the lines inside it.
      const indent = line.length - line.trimStart().length;
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) body.push(lines[i++].replace(/^ +/, (sp) => sp.slice(indent)));
      // A block still being written (a reply as it streams) has no closing fence yet.
      const closed = i < lines.length;
      i++;
      out.push({ kind: 'code', text: body.join('\n'), shell: closed && SHELLS.has(lang) });
      continue;
    }
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      out.push({ kind: 'heading', level: h[1].length, text: h[2] });
      i++;
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      out.push({ kind: 'rule' });
      i++;
      continue;
    }
    if (tableAt(lines, i)) {
      const head = cells(line);
      const align = cells(lines[i + 1]).map((d): Align => (d.startsWith(':') ? (d.endsWith(':') ? 'center' : 'left') : d.endsWith(':') ? 'right' : null));
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i].trim() && lines[i].includes('|')) {
        const r = cells(lines[i++]);
        // Rows pad or trim to the header's width, as GitHub's do.
        rows.push(head.map((_, j) => r[j] ?? ''));
      }
      out.push({ kind: 'table', align: head.map((_, j) => align[j] ?? null), head, rows });
      continue;
    }
    const item = ITEM.exec(line);
    if (item) {
      const ordered = !!item[1];
      const items: string[] = [];
      while (i < lines.length) {
        const m = ITEM.exec(lines[i]);
        if (m) items.push(m[2]);
        // A wrapped line continues the item before it; a fence under it is a code block of its own.
        else if (lines[i].trim() && /^\s{2,}/.test(lines[i]) && !/^\s*```/.test(lines[i]) && items.length) items[items.length - 1] += ` ${lines[i].trim()}`;
        else break;
        i++;
      }
      out.push(ordered ? { kind: 'list', ordered, items, start: Number(item[1]) } : { kind: 'list', ordered, items });
      continue;
    }
    if (/^\s*>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ''));
      out.push({ kind: 'quote', text: body.join(' ') });
      continue;
    }
    const body: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6})\s|^\s*```|^\s*>/.test(lines[i]) && !ITEM.exec(lines[i]) && !tableAt(lines, i)) body.push(lines[i++].trim());
    out.push({ kind: 'para', text: body.join(' ') });
  }
  return out;
}

/** A line's inline marks as elements: code first, so nothing inside backticks is read as a mark.
 *  Inside a link (`links` false) nothing more becomes a link, since links can't nest. */
export function inline(text: string, links = true): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|\[([^\]]+)\]\(([^)\s]+)[^)]*\)|<(https?:\/\/[^\s>]+)>|\*([^*\s][^*]*)\*/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let n = 0;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(...(links ? bare(text.slice(last, m.index), n++) : [text.slice(last, m.index)]));
    if (m[1] != null) out.push(<code key={n++} className="ca-ic">{m[1]}</code>);
    else if (m[2] != null || m[3] != null) out.push(<strong key={n++}>{inline(m[2] ?? m[3], links)}</strong>);
    else if (m[4] != null)
      out.push(
        links && WEB.test(m[5]) ? (
          <Link key={n++} url={m[5]}>
            {inline(m[4], false)}
          </Link>
        ) : (
          <Fragment key={n++}>{inline(m[4], links)}</Fragment>
        ),
      );
    else if (m[6] != null)
      out.push(
        links ? (
          <Link key={n++} url={m[6]}>
            {m[6]}
          </Link>
        ) : (
          m[6]
        ),
      );
    else out.push(<em key={n++}>{inline(m[7], links)}</em>);
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(...(links ? bare(text.slice(last), n++) : [text.slice(last)]));
  return out;
}

/** Memoized: a thread re-renders every clock tick, and its messages' text doesn't change. */
export const Markdown = memo(function Markdown({ text, className }: { text: string; className?: string }) {
  return (
    <div className={['ca-md', className].filter(Boolean).join(' ')}>
      {blocks(text).map((b, i) => {
        switch (b.kind) {
          case 'heading':
            return (
              <p key={i} className={`ca-md-h ca-md-h${Math.min(b.level, 3)}`}>
                {inline(b.text)}
              </p>
            );
          case 'para':
            return <p key={i}>{inline(b.text)}</p>;
          case 'list': {
            const Tag = b.ordered ? 'ol' : 'ul';
            return (
              <Tag key={i} start={b.start != null && b.start !== 1 ? b.start : undefined}>
                {b.items.map((it, j) => (
                  <li key={j}>{inline(it)}</li>
                ))}
              </Tag>
            );
          }
          case 'quote':
            return <blockquote key={i}>{inline(b.text)}</blockquote>;
          case 'code':
            return <CodeBlock key={i} text={b.text} run={b.shell ? {} : undefined} />;
          case 'table':
            return (
              <div key={i} className="ca-md-table">
                <table>
                  <thead>
                    <tr>
                      {b.head.map((c, j) => (
                        <th key={j} style={b.align[j] ? { textAlign: b.align[j] } : undefined}>
                          {inline(c)}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {b.rows.map((r, k) => (
                      <tr key={k}>
                        {r.map((c, j) => (
                          <td key={j} style={b.align[j] ? { textAlign: b.align[j] } : undefined}>
                            {inline(c)}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          case 'rule':
            return <hr key={i} />;
        }
      })}
    </div>
  );
});
