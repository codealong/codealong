// Just enough Markdown for what agents write back: headings, lists, quotes, fenced code, tables,
// and **bold**, *italics*, `code` and [links](url) inside lines. Built as React elements, never as
// HTML, so nothing in a transcript can put markup on the page. Web links, written as links or bare,
// open in the browser; any other link shows as its text. A link to a GitHub PR is the PR's
// (PrLink.tsx): a bare one reads `name#123`, and hovering it shows the PR and its sessions.
// A tool's output isn't Markdown, but the web links in it open the same way (`linked`).
// The directives Codex's app draws (directives.ts) are drawn as it draws them, near enough: a
// writing block as a card with Copy, a code comment as a card, a follow-up it offers as a button
// that puts its prompt in the session's box, a file it cites as the file. Another `:::name` block
// is a card of the same kind; other directives stay as they're written.

import { createContext, Fragment, memo, useContext, useState, type ReactNode } from 'react';
import { openUrl } from '../api';
import { ATTRS, attrsOf, CARD_CLOSE, CARD_OPEN, COMMENT, tonesOf } from '../directives';
import { tildePath } from '../format';
import { prLabel, prRefOf } from '../prLinks';
import { CodeBlock, CopyButton } from './CopyButton';
import { PrLink } from './PrLink';
import { cx, Icon, Seg } from './primitives';

const WEB = /^https?:\/\//i;
/** A bare web link in text, without the punctuation that ends a sentence around it. */
const BARE = /https?:\/\/[^\s<>()`]+[^\s<>()`.,;:!?'"*_]/g;

/** A web link in a tool's output, which is as often JSON as prose: it ends at a quote or a
 *  backslash too. */
const IN_OUTPUT = /https?:\/\/[^\s<>()`"'\\]+[^\s<>()`"'\\.,;:!?*_]/g;

/** A web link that opens in the browser, never in the app's own window. A bare one (`children`
 *  left out) reads as its URL, or a PR's as GitHub shortens it; `plain` keeps a PR's as it is. */
export function Link({ url, children, plain }: { url: string; children?: ReactNode; plain?: boolean }) {
  const pr = plain ? null : prRefOf(url);
  if (pr) {
    return (
      <PrLink url={url} pr={pr}>
        {children ?? prLabel(pr)}
      </PrLink>
    );
  }
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
      {children ?? url}
    </a>
  );
}

/** Plain text with its bare web links made links. */
function bare(text: string, key: number, output = false): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(output ? IN_OUTPUT : BARE)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push(<Link key={`${key}-${m.index}`} url={m[0]} plain={output} />);
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

/** A tool's output as it was written, every character of it, with its web links made links. */
export function linked(text: string): ReactNode[] {
  return bare(text, 0, true);
}

type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'para'; text: string }
  /** Each item is what's written in it: its first line's paragraph, then whatever is indented
   *  under it (a list nested in it, a paragraph, a code block). `start`: an ordered list's first
   *  number, when it isn't 1. */
  | { kind: 'list'; ordered: boolean; items: Block[][]; start?: number }
  | { kind: 'quote'; text: string }
  /** `shell`: a finished block of shell commands, which can run in a new terminal. `session`:
   *  one written for the session's own shell, led by `!` as Claude Code's box takes one (bash
   *  mode), which runs in the session where it can (CopyButton.tsx). `bare`: a finished block
   *  with no language, which runs as shell if each of its lines starts with a command
   *  (RunButton.tsx `isCommandBlock`). */
  | { kind: 'code'; text: string; shell: boolean; session: boolean; bare: boolean }
  | { kind: 'table'; align: Align[]; head: string[]; rows: string[][] }
  | { kind: 'rule' }
  /** A block a directive wraps (`:::name{…}` … `:::`): Codex's writing block, a finished piece of
   *  writing to use elsewhere (an email, a message, a document), or another's. One still being
   *  written has no `:::` yet and holds the rest. */
  | { kind: 'card'; name: string; attrs: Record<string, string>; text: string }
  /** A comment Codex attached to lines of a file (`::code-comment{…}`). */
  | { kind: 'comment'; attrs: Record<string, string> };

type Align = 'left' | 'center' | 'right' | null;

const ITEM = /^\s*(?:[-*+]|(\d+)[.)])\s+(.*)$/;
/** A fence's languages for shell commands. A console session isn't one: it holds prompts and
 *  output. A block with no language is as often output or a path as a command, so it's shell
 *  when it starts with `!` (a command for the session's own shell, as agents write one for you to
 *  run there) or when each of its lines starts with a command this Mac has. */
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

/** How far a line is indented, in characters. */
const indentOf = (line: string) => line.length - line.trimStart().length;

/** A list item's lines as Markdown of their own: its first line, then the lines under it with the
 *  indentation they share taken off, so a list nested in it reads as a list. */
function dedent(body: string[]): string {
  const [first, ...rest] = body;
  const cut = Math.min(...rest.filter((l) => l.trim()).map(indentOf));
  return [first, ...rest.map((l) => (l.trim() ? l.slice(cut) : ''))].join('\n');
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
      const text = body.join('\n');
      const session = closed && (lang === '' || SHELLS.has(lang)) && text.trimStart().startsWith('!');
      out.push({ kind: 'code', text, shell: (closed && SHELLS.has(lang)) || session, session, bare: closed && lang === '' && !session });
      continue;
    }
    const card = CARD_OPEN.exec(line);
    if (card) {
      const body: string[] = [];
      let fenced = false;
      i++;
      // A `:::` inside a code block in it is the code's, not its end.
      while (i < lines.length && (fenced || !CARD_CLOSE.test(lines[i]))) {
        if (/^\s*```/.test(lines[i])) fenced = !fenced;
        body.push(lines[i++]);
      }
      i++;
      out.push({ kind: 'card', name: card[1], attrs: attrsOf(card[2] ?? ''), text: body.join('\n') });
      continue;
    }
    const comment = COMMENT.exec(line);
    if (comment) {
      out.push({ kind: 'comment', attrs: attrsOf(comment[1]) });
      i++;
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
      const at = indentOf(line);
      const items: Block[][] = [];
      while (i < lines.length) {
        const m = ITEM.exec(lines[i]);
        if (!m || indentOf(lines[i]) !== at || !!m[1] !== ordered) break;
        // An item holds the lines indented further than its marker, blank lines between them
        // included: a wrapped line, a list nested in it, a paragraph or a code block under it.
        const body = [m[2]];
        i++;
        let fenced = false;
        while (i < lines.length) {
          // Inside a code block every line is the block's, however far it's indented.
          if (fenced || (lines[i].trim() && indentOf(lines[i]) > at)) {
            if (/^\s*```/.test(lines[i])) fenced = !fenced;
            body.push(lines[i++]);
            continue;
          }
          if (lines[i].trim()) break;
          const next = lines.slice(i).findIndex((l) => l.trim() !== '');
          if (next < 0 || indentOf(lines[i + next]) <= at) break;
          body.push(...lines.slice(i, i + next));
          i += next;
        }
        items.push(blocks(dedent(body)));
        // A blank line between items keeps them in one list.
        const next = lines.slice(i).findIndex((l) => l.trim() !== '');
        const sibling = next >= 0 ? ITEM.exec(lines[i + next]) : null;
        if (next > 0 && sibling && indentOf(lines[i + next]) === at && !!sibling[1] === ordered) i += next;
      }
      const start = Number(item[1]);
      out.push(ordered && start !== 1 ? { kind: 'list', ordered, items, start } : { kind: 'list', ordered, items });
      continue;
    }
    if (/^\s*>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ''));
      out.push({ kind: 'quote', text: body.join(' ') });
      continue;
    }
    const body: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6})\s|^\s*```|^\s*>/.test(lines[i]) && !ITEM.exec(lines[i]) && !tableAt(lines, i) && !CARD_OPEN.test(lines[i]) && !COMMENT.test(lines[i])) body.push(lines[i++].trim());
    out.push({ kind: 'para', text: body.join(' ') });
  }
  return out;
}

/** A line's inline marks, each its own group: code, bold, a link (words, then where), a bare link
 *  in angle brackets, a Codex follow-up (label, then attributes), a file Codex cites, italics. */
const INLINE = String.raw`\`([^\`]+)\`|\*\*([^*]+)\*\*|__([^_]+)__|\[([^\]]+)\]\(([^)\s]+)[^)]*\)|<(https?:\/\/[^\s>]+)>|(?<![\w:]):codex-followup\[([^\]]*)\]${ATTRS}|(?<![\w:]):codex-file-citation${ATTRS}|\*([^*\s][^*]*)\*`;

/** A line's inline marks as elements: code first, so nothing inside backticks is read as a mark.
 *  Inside a link (`links` false) nothing more becomes a link, since links can't nest. Codex's
 *  follow-ups (`:codex-followup[label]{prompt="…"}`) and the files it cites
 *  (`:codex-file-citation{path="…"}`) are drawn as theirs. */
export function inline(text: string, links = true): ReactNode[] {
  const out: ReactNode[] = [];
  const re = new RegExp(INLINE, 'g');
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
    else if (m[6] != null) out.push(links ? <Link key={n++} url={m[6]} /> : m[6]);
    else if (m[7] != null) out.push(<FollowUp key={n++} label={m[7]} prompt={attrsOf(m[8]).prompt ?? ''} />);
    else if (m[9] != null) out.push(<CitedFile key={n++} path={attrsOf(m[9]).path ?? ''} />);
    else out.push(<em key={n++}>{inline(m[10], links)}</em>);
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(...(links ? bare(text.slice(last), n++) : [text.slice(last)]));
  return out;
}

/** Where a follow-up Codex offers goes when it's clicked: into the box of the session whose
 *  conversation shows it (SessionThread.tsx). Elsewhere a follow-up is its words. */
export const FollowUpContext = createContext<((prompt: string) => void) | null>(null);

/** A follow-up Codex offers, as its app offers it to click: the prompt goes in the session's box,
 *  after what's there, to send or change. */
function FollowUp({ label, prompt }: { label: string; prompt: string }) {
  const put = useContext(FollowUpContext);
  if (!put || !prompt.trim()) {
    return (
      <span className="ca-md-followup" title={prompt || undefined}>
        {label}
      </span>
    );
  }
  return (
    <button
      type="button"
      className="ca-md-followup is-live"
      title={`Put in the box: ${prompt}`}
      onClick={(e) => {
        e.stopPropagation();
        put(prompt);
      }}
    >
      <Icon name="message" size={12} />
      <span>{label}</span>
    </button>
  );
}

/** A file Codex cites, as its app shows one: the file's name, then its folder. */
function CitedFile({ path }: { path: string }) {
  const cut = path.lastIndexOf('/');
  const name = path.slice(cut + 1) || path;
  const folder = cut > 0 ? tildePath(path.slice(0, cut)) : '';
  return (
    <span className="ca-md-file" title={path}>
      <Icon name="file" size={12} />
      <span className="ca-md-file-name">{name}</span>
      {folder && <span className="ca-md-file-dir">{folder}</span>}
    </span>
  );
}

/** What a writing block's header calls it, by its variant, when it has no title of its own. */
const WRITING: Record<string, { label: string; icon: string }> = {
  email: { label: 'Email', icon: 'mail' },
  chat_message: { label: 'Message', icon: 'message' },
  social_post: { label: 'Post', icon: 'message-circle' },
  document: { label: 'Document', icon: 'file-text' },
  slides: { label: 'Presentation outline', icon: 'layout' },
};

/** A block a directive wraps, as a card: what it is along the top with Copy, which copies the
 *  writing alone, without its fences; a writing block's versions to pick between, and an email's
 *  recipients and subject over its body. */
function DirectiveCard({ name, attrs, text }: { name: string; attrs: Record<string, string>; text: string }) {
  const tones = tonesOf(text);
  const [tone, setTone] = useState(0);
  const shown = tones[Math.min(tone, tones.length - 1)];
  const writing = name === 'writing';
  const kind = writing ? (WRITING[attrs.variant ?? ''] ?? { label: 'Writing', icon: 'pencil' }) : { label: name.replace(/[-_]+/g, ' ').replace(/^./, (c) => c.toUpperCase()), icon: 'note' };
  const fields = writing ? ([['To', attrs.recipient], ['Cc', attrs.cc], ['Bcc', attrs.bcc], ['Subject', attrs.subject]] as const).filter(([, v]) => v) : [];
  return (
    <div className="ca-md-card">
      <div className="ca-md-card-head">
        <Icon name={kind.icon} size={13} />
        <span className="ca-md-card-title">{attrs.title || kind.label}</span>
        {tones.length > 1 && <Seg className="ca-md-card-tones" label="Versions" options={tones.map((t, j) => ({ id: j, label: t.label || `Version ${j + 1}` }))} value={tone} onPick={setTone} />}
        {shown.text && <CopyButton text={shown.text} />}
      </div>
      {fields.length > 0 && (
        <dl className="ca-md-card-fields">
          {fields.map(([k, v]) => (
            <Fragment key={k}>
              <dt>{k}</dt>
              <dd>{v}</dd>
            </Fragment>
          ))}
        </dl>
      )}
      {shown.text && <div className="ca-md-card-body">{blocks(shown.text).map(drawBlock)}</div>}
    </div>
  );
}

/** A comment Codex attached to lines of a file, as a card: its title, where, and what it says. */
function CodeComment({ attrs }: { attrs: Record<string, string> }) {
  const { title, body, file, start, end } = attrs;
  const lines = start ? `:${start}${end && end !== start ? `–${end}` : ''}` : '';
  return (
    <div className="ca-md-card is-comment">
      <div className="ca-md-card-head">
        <Icon name="message" size={13} />
        <span className="ca-md-card-title">{inline(title || 'Comment')}</span>
      </div>
      {file && (
        <p className="ca-md-card-where" title={file}>
          {tildePath(file)}
          {lines}
        </p>
      )}
      {body && (
        <div className="ca-md-card-body">
          <p>{inline(body)}</p>
        </div>
      )}
    </div>
  );
}

/** A list item: its first line's paragraph as a line of the item, then what's under it (a nested
 *  list, a paragraph, a code block) as blocks. */
function drawItem(item: Block[], j: number): ReactNode {
  const [first, ...rest] = item;
  const lead = first?.kind === 'para' ? inline(first.text) : first ? drawBlock(first, 0) : null;
  return (
    <li key={j}>
      {lead}
      {rest.map((b, k) => drawBlock(b, k + 1))}
    </li>
  );
}

function drawBlock(b: Block, i: number): ReactNode {
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
        <Tag key={i} start={b.start}>
          {b.items.map(drawItem)}
        </Tag>
      );
    }
    case 'quote':
      return <blockquote key={i}>{inline(b.text)}</blockquote>;
    case 'code':
      return <CodeBlock key={i} text={b.text} run={b.shell ? { session: b.session } : b.bare ? { ifCommands: true } : undefined} />;
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
    case 'card':
      return <DirectiveCard key={i} name={b.name} attrs={b.attrs} text={b.text} />;
    case 'comment':
      return <CodeComment key={i} attrs={b.attrs} />;
  }
}

/** Memoized: a thread re-renders every clock tick, and its messages' text doesn't change. */
export const Markdown = memo(function Markdown({ text, className }: { text: string; className?: string }) {
  return <div className={cx('ca-md', className)}>{blocks(text).map(drawBlock)}</div>;
});
