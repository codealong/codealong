// The pages on the web a session made or showed you (`Session.artifacts`: artifacts Claude Code
// published, made from a type or opened), in its header: the latest as a chip among its links,
// and the rest in a list under them that a toggle beside the chip shows, as the sessions that
// came of it show under its title. A click on one opens it in the browser.

import { openUrl } from '../api';
import { ago, agoPhrase } from '../format';
import type { Artifact } from '../types';
import { Chip, Icon } from './primitives';

/** What a page's tooltip says of it: its title, what was last done with it, and when. */
export function artifactWords(a: Artifact, now: number): string {
  return [a.title, a.note, a.at ? agoPhrase(a.at, now) : null].filter(Boolean).join(' · ');
}

/** Opening it can only fail for a link the browser won't take, which the click shows nothing of
 *  either way. */
const open = (a: Artifact) => void openUrl(a.url).catch(() => {});

/** The latest page a session made, among its header's links. */
export function ArtifactChip({ artifact: a, now }: { artifact: Artifact; now: number }) {
  return (
    <Chip className="ca-artifactchip" title={artifactWords(a, now)} does="open it in the browser" act={() => open(a)}>
      <Icon name="app" size={12} />
      <span className="ca-artifactchip-title">{a.title}</span>
    </Chip>
  );
}

/** The rest of them, under its links: each its title, what was last done with it and how long
 *  ago, the latest first. */
export function ArtifactList({ artifacts, now }: { artifacts: Artifact[]; now: number }) {
  return (
    <div className="ca-artifacts" role="list" aria-label="Pages it made">
      {artifacts.map((a) => (
        <button key={a.url} type="button" role="listitem" className="ca-artifact" title={`${artifactWords(a, now)}. Click to open it in the browser.`} onClick={() => open(a)}>
          <Icon name="app" size={12} />
          <span className="ca-artifact-title">{a.title}</span>
          {a.note && <span className="ca-artifact-note">{a.note}</span>}
          {a.at > 0 && <span className="ca-artifact-age">{ago(a.at, now)}</span>}
        </button>
      ))}
    </div>
  );
}
