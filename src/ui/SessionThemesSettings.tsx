// Settings › Experimental controls for experimental session themes (src/sessionTheme.ts,
// src-tauri/src/backgrounds.rs): turns `[experimental] session_themes` on/off, optionally enables
// per-session AI backgrounds (`[backgrounds]`) drawn by Codex or Antigravity with a configurable
// global style prompt, lets you compare every fallback mesh vibe on live miniature cards, switch
// between whole-panel and header-only backdrops, dial intensity, and opt in to subtle Sessions
// list row accents.

import { useEffect, useState } from 'react';
import { DRAWS } from '../harness';
import {
  hashString,
  SESSION_INTENSITIES,
  SESSION_PALETTE,
  SESSION_SURFACES,
  SESSION_VIBES,
  type AssignedSessionTheme,
} from '../sessionTheme';
import type { BackgroundsCfg, Config } from '../types';
import { ProfileSelect } from './ProfileSelect';
import { SessionBackdrop } from './SessionBackdrop';
import { useSessionThemePrefs } from './SessionThemeProvider';
import { CheckBox, cx, Seg } from './primitives';

const BACKGROUNDS_OFF: BackgroundsCfg = { enabled: false, account: '', prompt: '' };

const DEFAULT_BG_PLACEHOLDER =
  'Minimalist fine-art editorial scene inspired by the session (Japanese woodblock ridges, botanical shadows on linen, frosted-glass light caustics, or gouache architecture), with calm low-contrast center negative space for crisp text and no words or borders.';

interface SessionThemesSectionProps {
  enabled: boolean;
  cfg: Config;
  disabled: boolean;
  onToggle: (on: boolean) => void;
  onSave: (next: Config) => void;
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

export function SessionThemesSection({ enabled, cfg, disabled, onToggle, onSave }: SessionThemesSectionProps) {
  const [prefs, setPrefs] = useSessionThemePrefs();
  const backgrounds = cfg.backgrounds ?? BACKGROUNDS_OFF;
  const [prompt, setPrompt] = useState(backgrounds.prompt);
  useEffect(() => setPrompt(backgrounds.prompt), [backgrounds.prompt]);

  const drawers = cfg.accounts.filter((a) => !a.disabled && DRAWS.includes(a.harness)).map((a) => a.name);
  const drawer = drawers.find((name) => name === backgrounds.account) ?? drawers.find((name) => name === cfg.emblems?.account) ?? drawers[0] ?? '';
  const saveBg = (patch: Partial<BackgroundsCfg>) => onSave({ ...cfg, backgrounds: { ...backgrounds, ...patch } });

  return (
    <section className="ca-settings-group">
      <label className="ca-field ca-settings-check ca-settings-experiment">
        <CheckBox checked={enabled} onChange={onToggle} />
        <span>Session themes</span>
        <small>
          Gives each session a distinct color chord and atmospheric backdrop on its header (and optionally behind its conversation) so sessions read apart at a glance. Click the color orb beside a session’s title to pick a palette, switch mesh vibes, or redraw its AI background.
        </small>
      </label>
      {enabled && (
        <>
          <label className="ca-field ca-settings-check">
            <CheckBox checked={backgrounds.enabled} disabled={disabled} onChange={(on) => saveBg({ enabled: on })} />
            <span>Draw an AI background for each session</span>
            <small>
              Automatically draws a bespoke fine-art background for each new top-level session and harmonizes its accent color with the artwork (skipping helpers, automatic PR reviews, and headless runs). The grainy mesh shows immediately while drawing.
              {drawers.length === 0 && ' Add a Codex or Antigravity account in Settings › Accounts to draw backgrounds.'}
            </small>
          </label>
          {backgrounds.enabled && drawers.length > 0 && (
            <label className="ca-field">
              <span>Drawn by</span>
              <ProfileSelect
                value={drawer}
                profiles={drawers.map((name) => ({ id: name, name }))}
                onChange={(account) => saveBg({ account })}
              />
              <small>Codex or Antigravity account whose image quota draws each session’s background.</small>
            </label>
          )}
          {backgrounds.enabled && (
            <label className="ca-field">
              <span>Global background prompt</span>
              <textarea
                rows={3}
                value={prompt}
                placeholder={DEFAULT_BG_PLACEHOLDER}
                disabled={disabled}
                onChange={(e) => setPrompt(e.target.value)}
                onBlur={() => prompt.trim() !== backgrounds.prompt.trim() && saveBg({ prompt: prompt.trim() })}
              />
              <small>
                Style instructions shared by every project unless overridden in Settings › Projects or the repo’s <code className="ca-ic">.codealong/config.toml</code> (<code className="ca-ic">[backgrounds] prompt</code> or <code className="ca-ic">prompt_file</code>).
              </small>
            </label>
          )}
          <div className="ca-field">
            <span>{backgrounds.enabled ? 'Fallback mesh vibe' : 'Vibe'}</span>
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
