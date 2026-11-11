// The pages on the web a session made or showed you (`Session.artifacts`: artifacts Claude Code
// published, made from a type or opened), in its header: the latest as a chip among its links,
// and the rest in a list under them that a toggle beside the chip shows, as the sessions that
// came of it show under its title. A click on one opens it in the browser: in the Chrome profile
// its account names, where it has one (engine.rs `open_link`). A right-click offers each of
// Chrome's profiles and the link copied (menus.ts `linkMenu`), and what went wrong there is said
// in place, as a conversation's links say it: the header has no other way to.

import { useState } from 'react';
import { openUrl } from '../api';
import { ago, agoPhrase } from '../format';
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

/** A page's menu, and why the last thing picked from it didn't happen, until the next try. A
 *  copied link needs no word. */
function useLinkMenu() {
  const [why, setWhy] = useState<string | null>(null);
  const menu = (a: Artifact) => {
    setWhy(null);
    showLinkMenu(a.url, { error: setWhy, info: () => {} });
  };
  const said = why ? (
    <span className="ca-artifact-why" role="status">
      {why}
    </span>
  ) : null;
  return { menu, said };
}

/** The latest page a session made, among its header's links. */
export function ArtifactChip({ artifact: a, now }: { artifact: Artifact; now: number }) {
  const { menu, said } = useLinkMenu();
  return (
    <>
      <Chip className="ca-artifactchip" title={artifactWords(a, now)} does="open it in the browser" act={() => open(a)} onMenu={() => menu(a)}>
        <Icon name="app" size={12} />
        <span className="ca-artifactchip-title">{a.title}</span>
      </Chip>
      {said}
    </>
  );
}

/** The rest of them, under its links: each its title, what was last done with it and how long
 *  ago, the latest first. */
export function ArtifactList({ artifacts, now }: { artifacts: Artifact[]; now: number }) {
  const { menu, said } = useLinkMenu();
  return (
    <div className="ca-artifacts" role="list" aria-label="Pages it made">
      {artifacts.map((a) => (
        <button
          key={a.url}
          type="button"
          role="listitem"
          className="ca-artifact"
          title={`${artifactWords(a, now)}. Click to open it in the browser.`}
          onClick={() => open(a)}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            menu(a);
          }}
        >
          <Icon name="app" size={12} />
          <span className="ca-artifact-title">{a.title}</span>
          {a.note && <span className="ca-artifact-note">{a.note}</span>}
          {a.at > 0 && <span className="ca-artifact-age">{ago(a.at, now)}</span>}
        </button>
      ))}
      {said}
    </div>
  );
}
