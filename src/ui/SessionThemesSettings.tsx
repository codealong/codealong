// Settings › Experimental controls for experimental session themes (src/sessionTheme.ts):
// turns `[experimental] session_themes` on/off and lets you compare every vibe on live miniature
// cards, switch between header-only and full conversation backdrop, dial intensity, and opt in to
// subtle Sessions list row accents.

import {
  hashString,
  SESSION_INTENSITIES,
  SESSION_PALETTE,
  SESSION_SURFACES,
  SESSION_VIBES,
  type AssignedSessionTheme,
} from '../sessionTheme';
import { SessionBackdrop } from './SessionBackdrop';
import { useSessionThemePrefs } from './SessionThemeProvider';
import { CheckBox, cx, Seg } from './primitives';

interface SessionThemesSectionProps {
  enabled: boolean;
  onToggle: (on: boolean) => void;
}

const SAMPLE_THEMES: Record<string, AssignedSessionTheme> = Object.fromEntries(
  SESSION_VIBES.map((v, i) => [
    v.id,
    {
      color: SESSION_PALETTE[(i * 2) % SESSION_PALETTE.length],
      source: 'emblem',
      seed: hashString(`preview-vibe-${v.id}`),
      emblemImage: null,
    },
  ]),
);

export function SessionThemesSection({ enabled, onToggle }: SessionThemesSectionProps) {
  const [prefs, setPrefs] = useSessionThemePrefs();
  return (
    <section className="ca-settings-group">
      <label className="ca-field ca-settings-check ca-settings-experiment">
        <CheckBox checked={enabled} onChange={onToggle} />
        <span>Session themes</span>
        <small>
          Gives each session a collision-free color matched to its emblem by default, with a generative backdrop on its header (and optionally behind its conversation) so sessions read apart at a glance. Click the color dot beside a session’s title to pick a color or switch vibes live.
        </small>
      </label>
      {enabled && (
        <>
          <div className="ca-field">
            <span>Vibe</span>
            <div className="ca-st-vibegrid" role="radiogroup" aria-label="Session theme vibe">
              {SESSION_VIBES.map((v) => {
                const active = prefs.vibe === v.id;
                return (
                  <button
                    key={v.id}
                    type="button"
                    role="radio"
                    aria-checked={active}
                    className={cx('ca-st-vibecard', active && 'is-on')}
                    onClick={() => setPrefs({ vibe: v.id })}
                  >
                    <div className="ca-st-vibethumb">
                      <SessionBackdrop theme={SAMPLE_THEMES[v.id]} vibe={v.id} intensity={prefs.intensity} place="preview" />
                      <span className="ca-st-vibeline" aria-hidden="true" />
                      <span className="ca-st-vibeline is-short" aria-hidden="true" />
                    </div>
                    <span className="ca-st-vibename">{v.label}</span>
                  </button>
                );
              })}
            </div>
            <small>{SESSION_VIBES.find((v) => v.id === prefs.vibe)?.says}</small>
          </div>
          <div className="ca-field">
            <span>Where it shows</span>
            <Seg
              label="Where session themes show"
              options={SESSION_SURFACES.map((s) => ({ id: s.id, label: s.label }))}
              value={prefs.surface}
              onPick={(surface) => setPrefs({ surface })}
            />
            <small>{SESSION_SURFACES.find((s) => s.id === prefs.surface)?.says}</small>
          </div>
          <div className="ca-field">
            <span>Strength</span>
            <Seg
              label="Session theme strength"
              options={SESSION_INTENSITIES.map((i) => ({ id: i.id, label: i.label }))}
              value={prefs.intensity}
              onPick={(intensity) => setPrefs({ intensity })}
            />
          </div>
          <label className="ca-field ca-settings-check">
            <CheckBox checked={prefs.listRows} onChange={(listRows) => setPrefs({ listRows })} />
            <span>Accent rows in the Sessions list</span>
            <small>Adds a quiet color rail and faint tint to each session row in the main window’s list. Urgent amber edges still take priority when a session needs you.</small>
          </label>
        </>
      )}
    </section>
  );
}
