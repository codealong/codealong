// The pages on the web a session made or showed you (`Session.artifacts`: artifacts Claude Code
// published, made from a type or opened), in its header: a chip each among its links, the latest
// first, the first few and then a "+N" as its PRs are (SessionHeader.tsx). What was last done with
// one and when is said on hover. A click on one opens it in the browser: in the Chrome profile its
// account names, where it has one (engine.rs `open_link`). A right-click offers each of Chrome's
// profiles and the link copied (menus.ts `linkMenu`), and what went wrong there is said in place,
// as a conversation's links say it: the header has no other way to.

import { useState } from 'react';
import { openUrl } from '../api';
import { agoPhrase } from '../format';
import { showLinkMenu } from '../menus';
import type { Artifact } from '../types';
import { Chip, Icon } from './primitives';

/** What a page's tooltip says of it: its title, what was last done with it, and when. */
export function artifactWords(a: Artifact, now: number): string {
  return [a.title, a.note, a.at ? agoPhrase(a.at, now) : null].filter(Boolean).join(' · ');
}

/** Opening it can only fail for a link the browser won't take, which the click shows nothing of
 *  either way. */
const open = (a: Artifact) => void openUrl(a.url).catch(() => {});

/** A page a session made, among its header's links. Why the last thing picked from its menu
 *  didn't happen is said after it, until the next try; a copied link needs no word. */
export function ArtifactChip({ artifact: a, now }: { artifact: Artifact; now: number }) {
  const [why, setWhy] = useState<string | null>(null);
  const menu = () => {
    setWhy(null);
    showLinkMenu(a.url, { error: setWhy, info: () => {} });
  };
  return (
    <>
      <Chip className="ca-artifactchip" title={artifactWords(a, now)} does="open it in the browser" act={() => open(a)} onMenu={menu}>
        <Icon name="app" size={12} />
        <span className="ca-artifactchip-title">{a.title}</span>
      </Chip>
      {why && (
        <span className="ca-artifact-why" role="status">
          {why}
        </span>
      )}
    </>
  );
}
