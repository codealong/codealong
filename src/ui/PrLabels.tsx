// A PR's labels as its repo on GitHub defines them: each a label chip, as a todo's are, its dot in
// the label's own color on GitHub, so it reads as it does there. A repo picks those colors, light
// or dark, so the dot keeps an edge of its own against the chip in either scheme. In a PR row they
// follow the title and show whole or not at all; elsewhere they wrap.

import type { CSSProperties } from 'react';
import type { PrLabel } from '../types';
import { Chip } from './primitives';

export function PrLabels({ labels }: { labels?: PrLabel[] }) {
  if (!labels?.length) return null;
  return (
    <span className="ca-prlabels">
      {labels.map((l) => (
        <Chip key={l.name} kind="label" className="ca-prlabel" title={l.description ? `${l.name}: ${l.description}` : l.name}>
          <i className="ca-labeldot ca-prlabel-dot" style={l.color ? ({ '--prlabel': `#${l.color}` } as CSSProperties) : undefined} aria-hidden="true" />
          <span className="ca-prlabel-name">{l.name}</span>
        </Chip>
      ))}
    </span>
  );
}
