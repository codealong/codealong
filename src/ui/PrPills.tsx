// A PR's standing as colored pills (see prStatus.ts): its review, its checks and anything else
// in the way of a merge, or one "Ready to merge".

import { prPills, type PrFacts } from '../prStatus';
import { Icon } from './primitives';

export function PrPills({ pr }: { pr: PrFacts }) {
  return (
    <span className="ca-prpills">
      {prPills(pr).map((p) => (
        <span key={p.key} className={`ca-pill ca-prpill is-${p.tone}`} title={p.title}>
          {p.icon && <Icon name={p.icon} size={12} stroke={p.icon === 'merge' ? 2 : 2.25} />}
          <span className="ca-prpill-text">{p.text}</span>
        </span>
      ))}
    </span>
  );
}
