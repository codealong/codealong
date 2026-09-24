// A PR's standing as colored pills (see prStatus.ts): its review, its checks and anything else
// in the way of a merge, or one "Ready to merge". In a PR row short of room they say less.

import { prPills, type PrFacts } from '../prStatus';
import { Icon } from './primitives';

export function PrPills({ pr }: { pr: PrFacts }) {
  const pills = prPills(pr);
  return (
    <span className="ca-prpills" data-count={pills.length}>
      {pills.map((p) => (
        <span key={p.key} className={['ca-pill', 'ca-prpill', `ca-prpill--${p.key}`, `is-${p.tone}`, p.short !== null && 'has-short'].filter(Boolean).join(' ')} title={p.title}>
          {p.icon && <Icon name={p.icon} size={12} stroke={p.icon === 'x' || p.icon === 'check' ? 2.25 : 2} />}
          <span className="ca-prpill-text">{p.text}</span>
          {p.short && <span className="ca-prpill-short">{p.short}</span>}
        </span>
      ))}
    </span>
  );
}
