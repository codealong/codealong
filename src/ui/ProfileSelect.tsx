// A select over named choices (`Profile`s: an app's profiles, a routing group, a decision model),
// as Settings and the setup wizard show one.

import type { Profile } from '../types';
import { Icon } from './primitives';

/** A select over the built-in profiles. A value the file names that isn't one stays listed,
 *  so a hand-edited setting shows as written instead of jumping to another app. */
export function ProfileSelect({ value, profiles, onChange }: { value: string; profiles: Profile[]; onChange: (id: string) => void }) {
  const options = profiles.some((p) => p.id === value) ? profiles : [...profiles, { id: value, name: value }];
  return (
    <div className="ca-start-select ca-settings-select">
      <select value={value} onChange={(e) => e.target.value !== value && onChange(e.target.value)}>
        {options.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
      <Icon name="chevron-down" size={12} />
    </div>
  );
}
