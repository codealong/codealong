// Just enough Markdown for what agents write back: headings, lists, quotes, fenced code, tables,
// and **bold**, *italics*, `code` and [links](url) inside lines. Your prompts are drawn with it too,
// kept as you typed them (`written`): their line breaks and indentation stay, and a glob written
// without backticks isn't read as italics (emphasis.ts). Built as React elements, never as
// HTML, so nothing in a transcript can put markup on the page. Web links, written as links or bare,
// open in the browser; a file or an image Codealong keeps (`[name](files/…)`, `![image](images/…)`,
// as a message sent with attachments refers to them) shows as a chip that opens it, or the image
// in place, on lines of its own (`paragraph`); any other link shows as its text. A full path written as code
// opens its file on a click once the engine has said it's there (`PathCode`, filePaths.ts), and a
// right-click on it, or on any file shown, offers your editor and the Finder too. A link to a GitHub PR is the PR's
// (PrLink.tsx): a bare one reads `name#123`, hovering it shows the PR and its sessions, and a
// click shows the PR in Codealong. A link to a session or a todo (`codealong://session/<key>`,
// codealongLinks.ts) is the session's or the todo's (SessionLink.tsx): a bare one reads as its
// title, and a session's key or a todo's id written as code is one too, as written. A tool's
// output isn't Markdown, but the web links in it open the same way (`linked`), a PR's keeping
// its words as written.
// The directives Codex's app draws (directives.ts) are drawn as it draws them, near enough: a
// writing block as a card with Copy, a code comment as a card, a follow-up it offers as a button
// that puts its prompt in the session's box, a file it cites as the file, which a click opens
// (`FileLink`). Another `:::name` block
// is a card of the same kind; other directives stay as they're written.

import { createContext, Fragment, isValidElement, memo, useContext, useState, type MouseEvent, type ReactNode } from 'react';
import { editFile, openFile, openUrl, revealFile, yourEditor } from '../api';
import { BARE_SOURCE, codealongLinkOf, codealongNameOf, type CodealongLink } from '../codealongLinks';
import { showContextMenu } from '../contextMenu';
import { pathIn, usePathKind, type PathAt } from '../filePaths';
import { fileMenu } from '../menus';
import type { PathKind } from '../types';
import { ATTRS, attrsOf, cardAt, cardFields, cardKind, COMMENT, opensCard, tonesOf, toneLabel, type Card } from '../directives';
import { BOLD, BOLD_UNDER, ITALIC } from '../emphasis';
import { tildePath } from '../format';
import { prLabel, prRefOf } from '../prLinks';
import { isKeptFile, keptImageOf } from '../todos';
import { CodeBlock, CopyButton } from './CopyButton';
import { errorText } from '../report';
import { FileChip, ImagesFrom, KeptImage } from './DescriptionEditor';
import { PrLink } from './PrLink';
import { cx, Icon, Seg } from './primitives';
import { SessionLink, TodoLink } from './SessionLink';

const WEB = /^https?:\/\//i;
/** A bare web link in text, without the punctuation that ends a sentence around it. */
const BARE = /https?:\/\/[^\s<>()`]+[^\s<>()`.,;:!?'"*_]/g;

/** A bare link in a reply's text: a web link, or one of Codealong's to a session or a todo. */
const BARE_OR_OWN = new RegExp(`${BARE.source}|${BARE_SOURCE}`, 'g');

/** A web link in a tool's output, which is as often JSON as prose: it ends at a quote or a
 *  backslash too. */
const IN_OUTPUT = /https?:\/\/[^\s<>()`"'\\]+[^\s<>()`"'\\.,;:!?*_]/g;

/** A link to a session or a todo, as SessionLink.tsx draws one. `words` left out, it reads as the
 *  session's title or the todo's; `unknown` is what shows where the window knows no such session
 *  or todo, and `plain` keeps the words as they are, with nothing ahead of them. */
function OwnLink({ to, words, unknown, plain }: { to: CodealongLink; words?: ReactNode; unknown: ReactNode; plain?: boolean }) {
  return to.kind === 'session' ? (
    <SessionLink name={to.name} unknown={unknown} plain={plain}>
      {words}
    </SessionLink>
  ) : (
    <TodoLink id={to.id} unknown={unknown} plain={plain}>
      {words}
    </TodoLink>
  );
}

/** A web link that opens in the browser, never in the app's own window, but for a PR's, which
 *  shows in Codealong (PrLink.tsx). A bare one (`children` left out) reads as its URL, or a PR's as
 *  GitHub shortens it; `plain` keeps a PR's as it is. */
export function Link({ url, children, plain }: { url: string; children?: ReactNode; plain?: boolean }) {
  const pr = prRefOf(url);
  if (pr) {
    return (
      <PrLink url={url} pr={pr} plain={plain}>
        {children ?? (plain ? url : prLabel(pr))}
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

/** Plain text with its bare links made links: web links, and in a reply's text Codealong's own to
 *  a session or a todo. */
function bare(text: string, key: number, output = false): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(output ? IN_OUTPUT : BARE_OR_OWN)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const own = output ? null : codealongLinkOf(m[0]);
    out.push(own ? <OwnLink key={`${key}-${m.index}`} to={own} unknown={m[0]} /> : <Link key={`${key}-${m.index}`} url={m[0]} plain={output} />);
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
  /** `shell`: a finished block of shell commands, which can run in a new terminal for the
   *  session. `session`: one written for the session's own shell, led by `!` as Claude Code's box
   *  takes one (bash mode), which runs as the box runs it (CopyButton.tsx). `bare`: a finished block
   *  with no language, which runs as shell if each of its lines starts with a command
   *  (RunButton.tsx `isCommandBlock`). */
  | { kind: 'code'; text: string; shell: boolean; session: boolean; bare: boolean }
  | { kind: 'table'; align: Align[]; head: string[]; rows: string[][] }
  | { kind: 'rule' }
  /** A block a directive wraps (`:::name{…}` … `:::`): Codex's writing block, a finished piece of
   *  writing to use elsewhere (an email, a message, a document), or another's. One still being
   *  written has no `:::` yet and holds the rest. */
  | ({ kind: 'card' } & Card)
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

/** `written`: a prompt's, kept as it was typed (`Markdown`'s `written`): a paragraph's or a quote's
 *  lines stay lines, each with its indentation, where an agent's reply joins them into one. */
export function blocks(src: string, written = false): Block[] {
  const out: Block[] = [];
  const join = (body: string[]) => body.join(written ? '\n' : ' ');
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
    const card = cardAt(lines, i);
    if (card) {
      out.push({ kind: 'card', ...card.card });
      i = card.next;
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
        items.push(blocks(dedent(body), written));
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
      out.push({ kind: 'quote', text: join(body) });
      continue;
    }
    const body: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6})\s|^\s*```|^\s*>/.test(lines[i]) && !ITEM.exec(lines[i]) && !tableAt(lines, i) && !opensCard(lines[i]) && !COMMENT.test(lines[i])) {
      const line = lines[i++];
      body.push(written ? line.trimEnd() : line.trim());
    }
    out.push({ kind: 'para', text: join(body) });
  }
  return out;
}

/** A line's inline marks, each its own group: code, bold, a link (words, then where), a bare link
 *  in angle brackets, a Codex follow-up (label, then attributes), a file Codex cites, italics, an
 *  image (words, then where). */
const INLINE = String.raw`\`([^\`]+)\`|${BOLD}|${BOLD_UNDER}|\[([^\]]+)\]\(([^)\s]+)[^)]*\)|<(https?:\/\/[^\s>]+)>|(?<![\w:]):codex-followup\[([^\]]*)\]${ATTRS}|(?<![\w:]):codex-file-citation${ATTRS}|${ITALIC}|!\[([^\]\n]*)\]\(([^)\s]+)\)`;

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
    if (m[1] != null) {
      const at = links ? pathIn(m[1]) : null;
      const named = links && !at ? codealongNameOf(m[1]) : null;
      if (at) out.push(<PathCode key={n++} code={m[1]} at={at} />);
      else if (named) out.push(<OwnLink key={n++} to={named} words={<code className="ca-ic ca-ic-path">{m[1]}</code>} unknown={<code className="ca-ic">{m[1]}</code>} plain />);
      else out.push(<code key={n++} className="ca-ic">{m[1]}</code>);
    }
    else if (m[2] != null || m[3] != null) out.push(<strong key={n++}>{inline(m[2] ?? m[3], links)}</strong>);
    else if (m[4] != null) {
      const own = links ? codealongLinkOf(m[5]) : null;
      out.push(
        links && WEB.test(m[5]) ? (
          <Link key={n++} url={m[5]}>
            {inline(m[4], false)}
          </Link>
        ) : own ? (
          <OwnLink key={n++} to={own} words={inline(m[4], false)} unknown={inline(m[4], links)} />
        ) : links && isKeptFile(m[5]) ? (
          <FileChip key={n++} name={m[4]} path={m[5]} />
        ) : (
          <Fragment key={n++}>{inline(m[4], links)}</Fragment>
        ),
      );
    }
    else if (m[6] != null) out.push(links ? <Link key={n++} url={m[6]} /> : m[6]);
    else if (m[7] != null) out.push(<FollowUp key={n++} label={m[7]} prompt={attrsOf(m[8]).prompt ?? ''} />);
    else if (m[9] != null) out.push(<FileLink key={n++} path={attrsOf(m[9]).path ?? ''} opens={links} />);
    else if (m[10] != null) out.push(<em key={n++}>{inline(m[10], links)}</em>);
    else {
      // Any other image reads as it did before images were drawn: its `!`, then its link.
      const kept = keptImageOf(m[12]);
      out.push(kept ? <KeptImage key={n++} name={kept} alt={m[11]} /> : <Fragment key={n++}>!{inline(m[0].slice(1), links)}</Fragment>);
    }
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

/** What a file a conversation names does, as `FileLink` and `PathCode` have it: a click opens it,
 *  in your editor at its line where the path gave one, else in the app that opens its kind
 *  (launch.rs `open_file`, which shows in the Finder what opening would run); a right-click
 *  offers each way, and its path (menus.ts `fileMenu`). What goes wrong is said in place (`why`):
 *  a file in a scratch folder is often gone by the time you look. */
function useFileActions({ path, line }: PathAt, kind: PathKind) {
  const [why, setWhy] = useState<string | null>(null);
  const tried = (work: Promise<void>) => {
    setWhy(null);
    work.catch((err: unknown) => setWhy(errorText(err)));
  };
  const open = () => tried(line != null && kind === 'file' ? editFile(path, line) : openFile(path));
  const menu = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // A copied path needs no word; one that couldn't be copied says so with the rest.
    const report = { error: setWhy, info: () => {} };
    tried(
      yourEditor().then((editor) =>
        showContextMenu(fileMenu(path, kind, { report, editor, open: () => tried(openFile(path)), edit: () => tried(editFile(path, line)), reveal: () => tried(revealFile(path)) })),
      ),
    );
  };
  return { why, open, menu };
}

/** Why a file didn't open, beside it. */
function FileWhy({ why }: { why: string | null }) {
  return why ? (
    <span className="ca-md-file-why" role="status">
      {why}
    </span>
  ) : null;
}

/** Whether `el` holds some of what's selected: a drag across a path to select it ends in a click
 *  on it, which shouldn't open it. */
function selectedIn(el: Element): boolean {
  const s = window.getSelection();
  return !!s && !s.isCollapsed && s.containsNode(el, true);
}

/** A path written as code (`/…` or `~/…`, filePaths.ts `pathIn`), as it's written, which opens
 *  once the engine has said something is there (`usePathKind`); until then, and for a session on
 *  another device, whose files are there, it's plain code. */
function PathCode({ code, at }: { code: string; at: PathAt }) {
  const away = useContext(ImagesFrom);
  const kind = usePathKind(away ? null : at.path);
  const f = useFileActions(at, kind ?? 'file');
  if (!kind) return <code className="ca-ic">{code}</code>;
  const title = kind === 'folder' ? `Reveal ${at.path} in Finder` : at.line != null ? `Open ${at.path} at line ${at.line} in your editor` : `Open ${at.path}`;
  return (
    <>
      <code
        className="ca-ic ca-ic-path"
        role="link"
        tabIndex={0}
        title={title}
        onClick={(e) => {
          if (selectedIn(e.currentTarget)) return;
          e.stopPropagation();
          f.open();
        }}
        onKeyDown={(e) => {
          if (e.key !== 'Enter') return;
          e.preventDefault();
          e.stopPropagation();
          f.open();
        }}
        onContextMenu={f.menu}
      >
        {code}
      </code>
      <FileWhy why={f.why} />
    </>
  );
}

/** A file on the session's Mac, as Codex's app shows one it cites: the file's name, then its
 *  folder. A file Codex cites and one a call sent you (SessionThread.tsx) both show this way, and
 *  open and offer their menu as a path written as code does (`useFileActions`). A session on
 *  another device has its files there, so they don't open here, nor does a path that isn't a
 *  full one; inside a link's text (`opens` false) it's only the name. */
export function FileLink({ path, opens = true }: { path: string; opens?: boolean }) {
  const away = useContext(ImagesFrom);
  const f = useFileActions({ path, line: null }, 'file');
  const cut = path.lastIndexOf('/');
  const name = path.slice(cut + 1) || path;
  const folder = cut > 0 ? tildePath(path.slice(0, cut)) : '';
  const shown = (
    <>
      <Icon name="file" size={12} />
      <span className="ca-md-file-name">{name}</span>
      {folder && <span className="ca-md-file-dir">{folder}</span>}
    </>
  );
  if (!opens || away || !path.startsWith('/')) {
    return (
      <span className="ca-md-file" title={away ? `${path}, on the session’s Mac` : path}>
        {shown}
      </span>
    );
  }
  return (
    <>
      <button
        type="button"
        className="ca-md-file is-open"
        title={`Open ${path}`}
        onClick={(e) => {
          e.stopPropagation();
          f.open();
        }}
        onContextMenu={f.menu}
      >
        {shown}
      </button>
      <FileWhy why={f.why} />
    </>
  );
}

/** A block a directive wraps, as a card: what it is along the top with Copy, which copies the
 *  writing alone, without its fences; a writing block's versions to pick between, and an email's
 *  recipients and subject over its body. */
function DirectiveCard({ card }: { card: Card }) {
  const tones = tonesOf(card.text);
  const [tone, setTone] = useState(0);
  const shown = tones[Math.min(tone, tones.length - 1)];
  const kind = cardKind(card);
  const fields = cardFields(card);
  return (
    <div className="ca-md-card">
      <div className="ca-md-card-head">
        <Icon name={kind.icon} size={13} />
        <span className="ca-md-card-title">{kind.label}</span>
        {tones.length > 1 && <Seg className="ca-md-card-tones" label="Versions" options={tones.map((t, j) => ({ id: j, label: toneLabel(t, j) }))} value={tone} onPick={setTone} />}
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

/** A paragraph. One with images Codealong keeps in it (a prompt's, which go on the lines after its
 *  words) shows its words and images one under another, as a description does (DescriptionEditor.tsx
 *  `DescriptionView`), rather than an image pressed against the line above it. */
function paragraph(text: string, key: number): ReactNode {
  const nodes = inline(text);
  const image = (n: ReactNode) => isValidElement(n) && n.type === KeptImage;
  if (!nodes.some(image)) return <p key={key}>{nodes}</p>;
  const out: ReactNode[] = [];
  let words: ReactNode[] = [];
  const said = () => {
    // The line breaks around an image are what put it on a line of its own; the words keep their
    // indentation, as a prompt's do.
    const edged = words.map((n, j) => {
      if (typeof n !== 'string') return n;
      const s = j === 0 ? n.replace(/^\n+/, '') : n;
      return j === words.length - 1 ? s.replace(/\n+$/, '') : s;
    });
    if (edged.some((n) => typeof n !== 'string' || n.trim() !== '')) out.push(<p key={`w${out.length}`}>{edged}</p>);
    words = [];
  };
  for (const n of nodes) {
    if (image(n)) {
      said();
      out.push(n);
    } else words.push(n);
  }
  said();
  return (
    <div key={key} className="ca-md-para">
      {out}
    </div>
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
      return paragraph(b.text, i);
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
      return <DirectiveCard key={i} card={b} />;
    case 'comment':
      return <CodeComment key={i} attrs={b.attrs} />;
  }
}

/** Memoized: a thread re-renders every clock tick, and its messages' text doesn't change.
 *  `written`: a prompt of yours, which shows as you typed it but for its marks: its line breaks
 *  and indentation stay, and a blank line between paragraphs is a line's height, as it was before
 *  prompts were drawn as Markdown, rather than a reply's narrower gap. */
export const Markdown = memo(function Markdown({ text, className, written = false }: { text: string; className?: string; written?: boolean }) {
  return <div className={cx('ca-md', written && 'is-written', className)}>{blocks(text, written).map(drawBlock)}</div>;
});
