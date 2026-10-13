// The directives Codex's app draws in a reply (codex.md has what its instructions to the model
// say): a writing block, a finished piece of writing to use elsewhere, wrapped as
// `:::writing{variant="email" id="83147" subject="…"}` … `:::`, with `---tone <label>` before
// each of two or three versions; a comment on lines of a file (`::code-comment{…}`); a follow-up
// to click (`:codex-followup[label]{prompt="…"}`); and a file it cites
// (`:codex-file-citation{path="…" purpose="output"}`). Markdown.tsx draws them, and the thread's
// find (threadFind.ts) reads them as they're drawn.

/** A directive's attributes, as `{variant="email" id="83147" start=10}` writes them; a quoted value
 *  may hold an escaped quote (`\"`). */
export function attrsOf(src: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of src.matchAll(/([A-Za-z][\w-]*)=(?:"((?:[^"\\]|\\.)*)"|([^\s"}]+))/g)) out[m[1]] = m[2] != null ? m[2].replace(/\\(.)/g, '$1') : m[3];
  return out;
}

/** A block a directive wraps opens with `:::name{attributes}` alone on its line and closes with
 *  `:::` alone on one, as Codex's writing blocks do. */
export const CARD_OPEN = /^\s*:::([A-Za-z][\w-]*)\s*(\{.*\})?\s*$/;
export const CARD_CLOSE = /^\s*:::\s*$/;
/** A comment Codex attaches to lines of a file, alone on its line:
 *  `::code-comment{title="[P2] Off-by-one" body="…" file="/path/foo.ts" start=10 end=11}`. */
export const COMMENT = /^\s*::code-comment(\{.*\})\s*$/;
/** A directive's attributes in a line, quoted values holding `}` and escaped quotes. */
export const ATTRS = String.raw`\{((?:[^}"]|"(?:[^"\\]|\\.)*")*)\}`;
const TONE = /^\s*---tone\s+(.+?)\s*$/;

/** A writing block's versions, each led by `---tone <label>` alone on its line, as Codex offers two
 *  or three of one piece; a block with none is one version without a label. */
export function tonesOf(text: string): { label: string; text: string }[] {
  const out: { label: string; text: string }[] = [];
  let label = '';
  let lines: string[] = [];
  const end = () => {
    const t = lines.join('\n').trim();
    if (label || t) out.push({ label, text: t });
  };
  for (const line of text.split('\n')) {
    const m = TONE.exec(line);
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

/** Markdown with Codex's directives as Markdown.tsx draws them, for reading as plain text: a
 *  block's fences gone and its versions by their labels, a code comment as its title, file and
 *  what it says, a follow-up as its words and a cited file as its path. */
export function plainDirectives(markdown: string): string {
  return markdown
    .split('\n')
    .map((line) => {
      if (CARD_OPEN.test(line) || CARD_CLOSE.test(line)) return '';
      const tone = TONE.exec(line);
      if (tone) return tone[1];
      const comment = COMMENT.exec(line);
      if (comment) {
        const a = attrsOf(comment[1]);
        return [a.title, a.file, a.body].filter(Boolean).join(' ');
      }
      return line;
    })
    .join('\n')
    .replace(new RegExp(String.raw`(?<![\w:]):codex-followup\[([^\]]*)\]${ATTRS}`, 'g'), '$1')
    .replace(new RegExp(String.raw`(?<![\w:]):codex-file-citation${ATTRS}`, 'g'), (_, attrs: string) => attrsOf(attrs).path ?? '');
}
