// Bold and italics as Markdown.tsx draws them, in replies and prompts alike, and as the thread's
// find (threadFind.ts) reads them, so a mark it doesn't draw is found as it shows.
//
// They open where a glob or a path written without backticks (`apps/*, libs/*`, `*.ts`,
// `Button*.tsx`, `src/**`) can't, so it reads as it's written, as prompts often have them: the
// marks come after no slash or other mark, and what they hold starts on something other than a
// space or a slash and ends on something other than a space. Italics come after no letter either
// (`2*3*4`) and don't start on the punctuation a glob goes on with (`*, `, `*.`), and underscores,
// which names are full of, come after and close before no letter. Bold can be part of a word
// (`light**house**`), as Markdown has it. Each pattern has one group, what the marks hold.

export const BOLD = String.raw`(?<![/\\*])\*\*([^\s*/](?:[^*]*[^\s*])?)\*\*`;
export const BOLD_UNDER = String.raw`(?<![\w/\\])__([^\s_/](?:[^_]*[^\s_])?)__(?!\w)`;
export const ITALIC = String.raw`(?<![\w/\\*])\*([^\s*/.,;:!?)\]}](?:[^*]*[^\s*])?)\*`;
