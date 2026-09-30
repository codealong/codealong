// Links to GitHub PRs in what's written in a conversation: which PR a link is, how a bare one
// reads (`name#123`, as GitHub shortens them), and the board's PRs by the keys links give.

import type { PrItem } from './types';

/** A link to a PR: its repo as the link names it, its number, and the part of it linked to. */
export interface PrRef {
  /** `owner/name`, as the link spells it. */
  slug: string;
  number: number;
  /** `owner/name#number` in lower case: GitHub's names don't go by case. */
  key: string;
  /** What within the PR the link goes to, when it isn't the PR itself. */
  part: 'files' | 'commits' | 'checks' | 'comment' | 'review' | null;
}

const PR_URL = /^https?:\/\/(?:www\.)?github\.com\/([\w.-]+)\/([\w.-]+)\/pull\/(\d+)(\/[^?#]*)?(\?[^#]*)?(#.*)?$/i;

export function prRefOf(url: string): PrRef | null {
  const m = PR_URL.exec(url);
  if (!m) return null;
  const [, owner, name, n, path = '', , hash = ''] = m;
  const slug = `${owner}/${name}`;
  const number = Number(n);
  const sub = path.split('/')[1]?.toLowerCase() ?? '';
  const part =
    /^#(issuecomment|discussion_r)/i.test(hash) ? 'comment'
    : /^#pullrequestreview/i.test(hash) ? 'review'
    : sub === 'files' || sub === 'commits' || sub === 'checks' ? sub
    : null;
  return { slug, number, key: `${slug}#${number}`.toLowerCase(), part };
}

/** A bare link's words: `name#123`, with what it goes to when that isn't the PR itself. */
export function prLabel(ref: PrRef): string {
  const name = ref.slug.slice(ref.slug.indexOf('/') + 1);
  return `${name}#${ref.number}${ref.part ? ` (${ref.part})` : ''}`;
}

/** The board's PRs by the keys links to them give: by the board's name for the repo and by the
 *  one in the PR's own URL, which is GitHub's name now when the repo was renamed. */
export function prIndex(prs: readonly PrItem[]): Map<string, PrItem> {
  const index = new Map<string, PrItem>();
  for (const p of prs) {
    index.set(p.key.toLowerCase(), p);
    const own = prRefOf(p.url);
    if (own && !index.has(own.key)) index.set(own.key, p);
  }
  return index;
}
