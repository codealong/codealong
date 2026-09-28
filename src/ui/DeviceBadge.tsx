// Which of your other devices something is on: a colored square and the device's name in a
// neutral chip, as labels are drawn, since the color names a device, not a status. This device's
// own worktrees and sessions carry none, so the usual case stays quiet.

import { deviceLabel } from '../devices';
import { agoPhrase } from '../format';
import type { DeviceTag } from '../types';

interface DeviceBadgeProps {
  device: Pick<DeviceTag, 'name' | 'color' | 'online' | 'lastSeen'>;
  now: number;
  /** The whole name, after "On", as a panel shows it; rows show the short one. */
  full?: boolean;
  /** The whole name alone, as a heading shows it. */
  heading?: boolean;
}

export function DeviceBadge({ device, now, full, heading }: DeviceBadgeProps) {
  const away = !device.online;
  const title = `On ${device.name}${away ? (device.lastSeen ? `, away: as of ${agoPhrase(device.lastSeen, now)}` : ', away') : ''}`;
  return (
    <span className={away ? 'ca-devbadge is-away' : 'ca-devbadge'} title={title}>
      <i className={`ca-labeldot is-${device.color}`} aria-hidden="true" />
      <span className="ca-devbadge-name">{heading ? device.name : full ? `On ${device.name}` : deviceLabel(device.name)}</span>
    </span>
  );
}
