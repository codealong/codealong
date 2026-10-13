// The directives Codex's app draws in a reply (codex.md has what its instructions to the model
// say): a writing block, a finished piece of writing to use elsewhere, wrapped as
// `:::writing{variant="email" id="83147" subject="…"}` … `:::`, with `---tone <label>` before
// each of two or three versions; a comment on lines of a file (`::code-comment{…}`); a follow-up
// to click (`:codex-followup[label]{prompt="…"}`); and a file it cites
// (`:codex-file-citation{path="…" purpose="output"}`). Markdown.tsx draws them, and the thread's
// find (threadFind.ts) reads them as they're drawn. Inside code they're code.

/** A directive's attributes, as `{variant="email" id="83147" start=10}` writes them. Codex escapes
 *  a quote in a quoted value (`\"`), and a backslash before one, and nothing else: `\d` in a
 *  prompt stays `\d`. */
export function attrsOf(src: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of src.matchAll(/([A-Za-z][\w-]*)=(?:"((?:[^"\\]|\\.)*)"|([^\s"}]+))/g)) out[m[1]] = m[2] != null ? m[2].replace(/\\(["\\])/g, '$1') : m[3];
  return out;
}

/** A block a directive wraps opens with `:::name{attributes}` alone on its line and closes with
 *  `:::` alone on one, as Codex's writing blocks do. */
const CARD_OPEN = /^\s*:::([A-Za-z][\w-]*)\s*(\{.*\})?\s*$/;
const CARD_CLOSE = /^\s*:::\s*$/;
/** A comment Codex attaches to lines of a file, alone on its line:
 *  `::code-comment{title="[P2] Off-by-one" body="…" file="/path/foo.ts" start=10 end=11}`. */
export const COMMENT = /^\s*::code-comment(\{.*\})\s*$/;
/** A directive's attributes in a line, quoted values holding `}` and escaped quotes. */
export const ATTRS = String.raw`\{((?:[^}"]|"(?:[^"\\]|\\.)*")*)\}`;
const TONE = /^\s*---tone\s+(.+?)\s*$/;
const FENCE = /^\s*```/;

/** Whether a line opens a block a directive wraps. */
export const opensCard = (line: string) => CARD_OPEN.test(line);

export interface Card {
  name: string;
  attrs: Record<string, string>;
  /** What's between its fences. One still being written has no `:::` yet and holds the rest. */
  text: string;
}

/** The block a directive wraps that opens at line i, and the line after it, or null. A `:::`
 *  inside a code block in it is the code's, not its end. */
export function cardAt(lines: string[], i: number): { card: Card; next: number } | null {
  const open = CARD_OPEN.exec(lines[i]);
  if (!open) return null;
  const body: string[] = [];
  let fenced = false;
  let j = i + 1;
  while (j < lines.length && (fenced || !CARD_CLOSE.test(lines[j]))) {
    if (FENCE.test(lines[j])) fenced = !fenced;
    body.push(lines[j++]);
  }
  return { card: { name: open[1], attrs: attrsOf(open[2] ?? ''), text: body.join('\n') }, next: j + 1 };
}

/** What a writing block's header calls it, by its variant, when it has no title of its own. */
const WRITING: Record<string, { label: string; icon: string }> = {
  email: { label: 'Email', icon: 'mail' },
  chat_message: { label: 'Message', icon: 'message' },
  social_post: { label: 'Post', icon: 'message-circle' },
  document: { label: 'Document', icon: 'file-text' },
  slides: { label: 'Presentation outline', icon: 'layout' },
};

/** What a card says it is along its top, and its icon: a writing block's title or variant,
 *  another directive's name. */
export function cardKind({ name, attrs }: Card): { label: string; icon: string } {
  const kind = name === 'writing' ? (WRITING[attrs.variant ?? ''] ?? { label: 'Writing', icon: 'pencil' }) : { label: name.replace(/[-_]+/g, ' ').replace(/^./, (c) => c.toUpperCase()), icon: 'note' };
  return attrs.title ? { ...kind, label: attrs.title } : kind;
}

/** An email's recipients and subject, as they show over its body. */
export function cardFields({ name, attrs }: Card): [string, string][] {
  if (name !== 'writing') return [];
  const fields: [string, string | undefined][] = [
    ['To', attrs.recipient],
    ['Cc', attrs.cc],
    ['Bcc', attrs.bcc],
    ['Subject', attrs.subject],
  ];
  return fields.filter((f): f is [string, string] => !!f[1]);
}

/** A writing block's versions, each led by `---tone <label>` alone on its line, as Codex offers two
 *  or three of one piece; a block with none is one version without a label. A line in a code
 *  block is the code's. */
export function tonesOf(text: string): { label: string; text: string }[] {
  const out: { label: string; text: string }[] = [];
  let label = '';
  let lines: string[] = [];
  let fenced = false;
  const end = () => {
    const t = lines.join('\n').trim();
    if (label || t) out.push({ label, text: t });
  };
  for (const line of text.split('\n')) {
    if (FENCE.test(line)) fenced = !fenced;
    const m = fenced ? null : TONE.exec(line);
    if (!m) {
      lines.push(line);
      continue;
    }
    end();
    label = m[1];
    lines = [];
  }
  end();
  return out.length > 0 ? out : [{ label: '', text: '' }];
}

/** What a version's button says. */
export const toneLabel = (t: { label: string }, j: number) => t.label || `Version ${j + 1}`;

const FOLLOW_UP = new RegExp(String.raw`(?<![\w:]):codex-followup\[([^\]]*)\]${ATTRS}`, 'g');
const CITED = new RegExp(String.raw`(?<![\w:]):codex-file-citation${ATTRS}`, 'g');

/** A reply with Codex's directives as Markdown.tsx draws them, for reading as plain text: a card
 *  as what its top says (what it is, its versions, an email's fields) and the version it shows
 *  first, a code comment as its title, file and what it says, a follow-up as its words and a
 *  cited file as its path. Code is left as it's written. */
export function plainDirectives(markdown: string): string {
  const lines = markdown.split('\n');
  const out: string[] = [];
  let fenced = false;
  for (let i = 0; i < lines.length; ) {
    const line = lines[i];
    if (FENCE.test(line)) fenced = !fenced;
    const at = fenced ? null : cardAt(lines, i);
    if (at) {
      const tones = tonesOf(at.card.text);
      const top = [cardKind(at.card).label, ...(tones.length > 1 ? tones.map(toneLabel) : []), ...cardFields(at.card).flat()];
      out.push(top.join(' '), plainDirectives(tones[0].text));
      i = at.next;
      continue;
    }
    const comment = fenced ? null : COMMENT.exec(line);
    if (comment) {
      const a = attrsOf(comment[1]);
      out.push([a.title || 'Comment', a.file, a.body].filter(Boolean).join(' '));
    } else if (fenced || FENCE.test(line)) {
      out.push(line);
    } else {
      // Inline code keeps what's in it.
      out.push(
        line
          .split(/(`[^`]+`)/)
          .map((part) => (part.startsWith('`') ? part : part.replace(FOLLOW_UP, '$1').replace(CITED, (_, attrs: string) => attrsOf(attrs).path ?? '')))
          .join(''),
      );
    }
    i++;
  }
  return out.join('\n');
}
