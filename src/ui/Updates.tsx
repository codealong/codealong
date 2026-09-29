// Automatic updates in the windows (src-tauri/src/updates.rs): the sidebar's Restart to update,
// once a newer version is in place, and the Updates field in Settings › General.
import { useEffect, useState } from 'react';
import { checkForUpdates, onUpdateStatus, restartToUpdate, updateStatus } from '../api';
import { agoPhrase } from '../format';
import type { Config, UpdateView } from '../types';
import { Button, Icon } from './primitives';

/** Where updates stand, kept current; `null` until the app has said (and outside it). */
export function useUpdates(): UpdateView | null {
  const [view, setView] = useState<UpdateView | null>(null);
  useEffect(() => {
    let live = true;
    void updateStatus()
      .then((v) => {
        if (live && v) setView(v);
      })
      .catch(() => {});
    const off = onUpdateStatus(setView);
    return () => {
      live = false;
      off();
    };
  }, []);
  return view;
}

/** Above the foot of the sidebar once a newer version is in place. The next launch runs it
 *  anyway; this is for now. */
export function RestartToUpdate({ compact }: { compact?: boolean }) {
  const view = useUpdates();
  if (view?.state !== 'ready') return null;
  // In the collapsed sidebar: its icon alone, marked.
  if (compact) {
    return (
      <button type="button" className="ca-rail-btn is-update" aria-label={`Restart to update to ${view.version}`} title={`Restart to update to ${view.version}`} onClick={() => void restartToUpdate()}>
        <Icon name="refresh" size={18} />
        <span className="ca-rail-badge is-ink" aria-hidden="true" />
      </button>
    );
  }
  return (
    <button type="button" className="ca-side-update" title={view.notes ?? `Codealong ${view.version} is ready`} onClick={() => void restartToUpdate()}>
      <Icon name="refresh" size={14} />
      <span>Restart to update to {view.version}</span>
    </button>
  );
}

/** What Settings says about updates. */
export function updateSays(view: UpdateView, now: number): string {
  switch (view.state) {
    case 'off':
      return 'This is a development build, which never updates itself.';
    case 'idle':
      return 'Codealong looks for a newer version a couple of minutes after it opens, then every six hours.';
    case 'checking':
      return 'Looking for a newer version…';
    case 'upToDate':
      return `Up to date. Checked ${agoPhrase(view.checkedAt, now)}.`;
    case 'installing':
      return `Getting Codealong ${view.version}…`;
    case 'ready':
      return `Codealong ${view.version} starts when you restart it${view.notes ? `. What's new: ${view.notes}` : '.'}`;
    case 'failed':
      return view.error;
  }
}

/** Settings › General: this copy's version, how updates stand, and whether to look by itself. */
export function UpdatesFields({ cfg, onSave, now }: { cfg: Config; onSave: (next: Config) => void; now: number }) {
  const view = useUpdates();
  if (!view) return null;
  const busy = view.state === 'checking' || view.state === 'installing';
  return (
    <>
      <div className="ca-field">
        <span>Version</span>
        <div className="ca-settings-version">
          <span>Codealong {view.current}</span>
          {view.state === 'ready' ? (
            <Button size="sm" icon="refresh" onClick={() => void restartToUpdate()}>
              Restart to update
            </Button>
          ) : (
            view.state !== 'off' && (
              <Button size="sm" disabled={busy} onClick={() => void checkForUpdates()}>
                Check now
              </Button>
            )
          )}
        </div>
        <small role="status">{updateSays(view, now)}</small>
      </div>
      <label className="ca-field ca-settings-check">
        <input type="checkbox" checked={cfg.updates?.check ?? true} onChange={(e) => onSave({ ...cfg, updates: { check: e.target.checked } })} />
        <span>Look for updates by itself</span>
        <small>A newer version is put in place for the next launch, and the sidebar offers to restart. The app menu's Check for updates… works either way.</small>
      </label>
    </>
  );
}
