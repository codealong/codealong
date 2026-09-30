// Light or dark, at the foot of the sidebar. The choice is the theme in config.toml, so every
// window follows it and it lasts.

import { useEffect } from 'react';
import type { Theme } from '../types';
import { Icon } from './primitives';

const THEMES: { id: Theme; label: string; icon: string }[] = [
  { id: 'system', label: 'Follow macOS', icon: 'device-desktop' },
  { id: 'light', label: 'Light', icon: 'sun' },
  { id: 'dark', label: 'Dark', icon: 'moon' },
];

/** Shows a theme on the page. `system` leaves it to prefers-color-scheme, which follows macOS;
 * `light` and `dark` set data-theme, which tokens.css puts ahead of macOS. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'system') delete root.dataset.theme;
  else root.dataset.theme = theme;
}

/** Keeps a window's page in `theme` as it changes (config.toml's `theme`). */
export function useTheme(theme: Theme) {
  useEffect(() => applyTheme(theme), [theme]);
}

export function ThemeSwitch({ theme, onPick }: { theme: Theme; onPick: (theme: Theme) => void }) {
  return (
    <div className="ca-segctl ca-theme" role="radiogroup" aria-label="Theme">
      {THEMES.map((t) => (
        <button
          key={t.id}
          type="button"
          role="radio"
          aria-checked={theme === t.id}
          aria-label={t.label}
          title={t.label}
          className={theme === t.id ? 'is-on' : undefined}
          onClick={() => theme !== t.id && onPick(t.id)}
        >
          <Icon name={t.icon} size={14} />
        </button>
      ))}
    </div>
  );
}
