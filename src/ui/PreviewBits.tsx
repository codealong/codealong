// A worktree's app: a chip on its row that opens it, a section in its panel, and the dialog that
// sets its URL and Chrome profile. The engine finds the server behind it (previews.rs): a
// process listening on a port whose folder is in the worktree.

import { useEffect, useState } from 'react';
import { getConfig, setPreview } from '../api';
import { absolute, duration } from '../format';
import type { ChromeProfile, Lane, Preview } from '../types';
import { Button, Chip, cx, Dialog, Icon } from './primitives';
import { errorText } from '../report';

/** A URL as rows show it: `feature.app.test`, `localhost:5173/admin`. */
export function previewHost(url: string): string {
  return url.replace(/^[a-z][a-z0-9+.-]*:\/\//i, '').replace(/\/$/, '');
}

/** Whether it runs, in a sentence: its chip's tooltip. */
export function previewState(p: Preview, now: number): string {
  const s = p.server;
  if (!s) return p.port != null ? `Not running: nothing in this worktree listens on port ${p.port}.` : 'Not running: nothing in this worktree’s folder serves it.';
  return `Running: ${s.name} on port ${s.port}, for ${duration(s.startedAt, now)}.`;
}

/** Where it opens, in a sentence, when that's a Chrome profile. */
function profileNote(p: Preview): string {
  return p.chromeProfile ? ` Opens in Chrome’s ${p.chromeProfile} profile.` : '';
}

/** The profile a setting names, as the engine finds it (chrome.rs): by name, then by the
 *  account signed in to it, then by folder, ignoring case. */
export function findChromeProfile(profiles: ChromeProfile[], wanted: string): ChromeProfile | undefined {
  const w = wanted.trim().toLowerCase();
  if (!w) return undefined;
  return profiles.find((p) => p.name.toLowerCase() === w) ?? profiles.find((p) => p.email?.toLowerCase() === w) ?? profiles.find((p) => p.dir.toLowerCase() === w);
}

/** A select over Chrome's profiles, with none first (`''`). A value Chrome doesn't list stays,
 *  so a hand-edited setting shows as written. */
export function ChromeProfileSelect({ value, profiles, none, onChange }: { value: string; profiles: ChromeProfile[]; none: string; onChange: (id: string) => void }) {
  const known = findChromeProfile(profiles, value);
  const shown = known ? known.id : value;
  return (
    <div className="ca-start-select ca-settings-select">
      <select value={shown} onChange={(e) => e.target.value !== shown && onChange(e.target.value)}>
        <option value="">{none}</option>
        {profiles.map((p) => (
          <option key={p.dir} value={p.id}>
            {p.email && p.email !== p.name ? `${p.name} · ${p.email}` : p.name}
          </option>
        ))}
        {value && !known && <option value={value}>{value} (Chrome has no such profile)</option>}
      </select>
      <Icon name="chevron-down" size={12} />
    </div>
  );
}

/** A worktree's app, wherever it shows (its row, a session's header, its panel): one chip that
 *  says where it opens and, by its color, whether it runs: green while a server in the worktree
 *  serves it, outlined when nothing does. On a list's row a double-click opens it, as a click
 *  selects the row; elsewhere a click does. */
export function PreviewChip({ preview: p, now, onOpen, inRow = false }: { preview: Preview | undefined; now: number; onOpen: () => void; inRow?: boolean }) {
  if (!p) return null;
  return (
    <Chip className={cx('ca-previewchip', p.server ? 'is-up' : 'is-down')} title={`${p.url}. ${previewState(p, now)}${profileNote(p)} ${inRow ? 'Double-click' : 'Click'} to open it.`} act={onOpen} inRow={inRow}>
      <Icon name="world" size={12} />
      <span className="ca-previewchip-host">{previewHost(p.url)}</span>
    </Chip>
  );
}

/** The worktree's app in its panel: where it opens, what serves it, and who started that. */
export function PreviewSection({ lane, now, onOpen, onEdit }: { lane: Lane; now: number; onOpen: () => void; onEdit: () => void }) {
  const p = lane.preview;
  if (!p) return null;
  const s = p.server;
  const by = s?.session ? lane.sessions.find((x) => x.key === s.session) : undefined;
  return (
    <section className="ca-insp-sec">
      <h4 className="ca-label">Preview</h4>
      <dl className="ca-kv">
        <dt>Opens at</dt>
        <dd>
          <button type="button" className="ca-preview-url" title={`Open ${p.url}`} onClick={onOpen}>
            {previewHost(p.url)}
          </button>
          {!p.set && <span className="ca-meta"> · found</span>}
        </dd>
        {p.chromeProfile && (
          <>
            <dt>Browser</dt>
            <dd>
              Chrome · {p.chromeProfile}
              <span className="ca-meta"> · {p.chromeProfileSet ? 'this worktree’s' : `${lane.project}’s`}</span>
            </dd>
          </>
        )}
        <dt>Server</dt>
        <dd className={s ? 'ca-preview-up' : 'ca-preview-down'}>
          {s ? (
            <>
              <span className="ca-code">{s.name}</span> on port {s.port}, pid {s.pid}
            </>
          ) : p.port != null ? (
            `Not running · port ${p.port}`
          ) : (
            'Not running'
          )}
        </dd>
        {s && (
          <>
            <dt>Up for</dt>
            <dd title={`Started ${absolute(s.startedAt)}`}>{duration(s.startedAt, now)}</dd>
          </>
        )}
        {s?.title && (
          <>
            <dt>Page</dt>
            <dd>{s.title}</dd>
          </>
        )}
        {by && (
          <>
            <dt>Started by</dt>
            <dd title={by.title}>
              {by.account} · {by.title}
            </dd>
          </>
        )}
      </dl>
      <div className="ca-preview-actions">
        <Button variant="ghost" size="sm" icon="world" onClick={onOpen}>
          Open
        </Button>
        <Button variant="ghost" size="sm" onClick={onEdit}>
          {p.set ? 'Change URL…' : 'Set URL…'}
        </Button>
      </div>
    </section>
  );
}

/** Sets where a worktree's app opens, which of its servers is the app, and the Chrome profile
 *  it opens in. */
export function PreviewDialog({ lane, onClose }: { lane: Lane; onClose: () => void }) {
  const p = lane.preview;
  const [url, setUrl] = useState(p?.set ? p.url : '');
  const [port, setPort] = useState(p?.port != null ? String(p.port) : '');
  // '' opens it in the project's profile, or the default browser when the project has none.
  const [profile, setProfile] = useState(p?.chromeProfileSet ? (p.chromeProfile ?? '') : '');
  const [chrome, setChrome] = useState<{ profiles: ChromeProfile[]; project: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let live = true;
    getConfig()
      .then((v) => live && setChrome({ profiles: v.chromeProfiles ?? [], project: v.config.projects.find((x) => x.name === lane.project)?.chrome_profile ?? null }))
      .catch(() => live && setChrome({ profiles: [], project: null }));
    return () => {
      live = false;
    };
  }, [lane.project]);

  const save = async (clear: boolean) => {
    const n = port.trim() === '' ? null : Number(port.trim());
    if (!clear) {
      if (url.trim() === '') return setError('Give the URL the worktree’s app opens at.');
      if (n !== null && !(Number.isInteger(n) && n > 0 && n < 65536)) return setError('A port is a whole number from 1 to 65535.');
    }
    setSaving(true);
    setError(null);
    try {
      await setPreview(lane.id, clear ? null : url.trim(), clear ? null : n, clear || !profile ? null : profile);
      onClose();
    } catch (e) {
      setError(errorText(e));
      setSaving(false);
    }
  };

  return (
    <Dialog
      className="ca-previewdlg"
      title="Preview URL"
      lede={`Where ${lane.callsign}’s app opens in the browser. Its row shows whether a server in the worktree’s folder is running it.`}
      busy={saving}
      onClose={onClose}
      onSubmit={() => void save(false)}
      aside={
        <span>
          {p?.set && (
            <Button variant="ghost" size="sm" onClick={() => void save(true)} disabled={saving}>
              Clear URL
            </Button>
          )}
        </span>
      }
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={saving}>
            {saving ? 'Saving…' : 'Save'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <label className="ca-field">
          <span>URL</span>
          <input value={url} autoFocus spellCheck={false} autoCapitalize="off" autoCorrect="off" placeholder="https://feature.app.test or localhost:5173" onChange={(e) => setUrl(e.target.value)} />
          <small>Without a scheme, localhost takes http and other names https.</small>
        </label>
        <label className="ca-field ca-previewdlg-port">
          <span>Port (optional)</span>
          <input value={port} inputMode="numeric" spellCheck={false} placeholder={p?.server ? String(p.server.port) : '5173'} onChange={(e) => setPort(e.target.value)} />
          <small>Which of the worktree’s servers runs the app. Leave it empty to use the one that serves a page, or the port a localhost URL names.</small>
        </label>
        {chrome && (chrome.profiles.length > 0 || profile || chrome.project) && (
          <label className="ca-field">
            <span>Chrome profile</span>
            <ChromeProfileSelect value={profile} profiles={chrome.profiles} none={chrome.project ? `${lane.project}’s: ${chrome.project}` : 'None: the default browser'} onChange={setProfile} />
            <small>Opens it where that profile’s sign-in and cookies are. A project’s profile, for all its worktrees, is set in Settings › Projects.</small>
          </label>
        )}
        {error && (
          <p className="ca-form-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </Dialog>
  );
}
