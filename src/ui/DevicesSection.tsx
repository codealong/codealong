// Settings › Devices: Codealong on your other Macs (src-tauri/src/peers.rs). Turning it on, how
// this Mac finds the others, the Macs it's linked with, linking more, and remote control. The
// switches are `[devices]` in config.toml; linking goes through the engine, which keeps the list.

import { useEffect, useState } from 'react';
import { answerLink, deviceLinkCode, getDevices, linkDevice, linkWithCode, onDevices, unlinkDevice } from '../api';
import { agoPhrase } from '../format';
import type { Config, DevicesCfg, DevicesView, DeviceView } from '../types';
import { DEVICES_OFF } from '../types';
import { Button } from './primitives';

interface DevicesPaneProps {
  cfg: Config;
  disabled: boolean;
  onSave: (next: Config) => Promise<boolean>;
  now: number;
}

export function DevicesPane({ cfg, disabled, onSave, now }: DevicesPaneProps) {
  const d: DevicesCfg = { ...DEVICES_OFF, ...cfg.devices };
  const set = (patch: Partial<DevicesCfg>) => void onSave({ ...cfg, devices: { ...d, ...patch } });
  const [view, setView] = useState<DevicesView | null>(null);
  const [name, setName] = useState(d.name);
  const [code, setCode] = useState<string | null>(null);
  const [entered, setEntered] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    void getDevices().then(setView);
    return onDevices(setView);
  }, []);
  useEffect(() => setName(d.name), [d.name]);

  const run = async (what: string, f: () => Promise<string | null>) => {
    setBusy(what);
    setNote(null);
    try {
      const said = await f();
      if (said) setNote({ ok: true, text: said });
    } catch (e) {
      setNote({ ok: false, text: String(e).replace(/^Error: /, '') });
    } finally {
      setBusy(null);
    }
  };

  const me = view?.me;
  return (
    <>
      <div className="ca-settings-intro">
        <h3>Devices</h3>
        <p>
          Codealong on each of your Macs shows the others' worktrees and sessions here, reads their conversations and searches what was said in them. Macs talk to each other directly, over your
          tailnet or through iroh, encrypted end to end; nothing goes through a server of Codealong's.
        </p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        <label className="ca-field ca-settings-check">
          <input type="checkbox" checked={d.enabled} onChange={(e) => set({ enabled: e.target.checked })} />
          <span>Show my other Macs</span>
          <small>While this is off, this Mac talks to no other.</small>
        </label>
        {d.enabled && (
          <>
            <label className="ca-field">
              <span>This Mac's name</span>
              <input
                value={name}
                placeholder={me?.name ?? 'The computer’s name'}
                spellCheck={false}
                onChange={(e) => setName(e.target.value)}
                onBlur={() => name.trim() !== d.name && set({ name: name.trim() })}
                onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
              />
              <small>What your other Macs call it. Empty uses the name in System Settings › General › Sharing.</small>
            </label>
            <label className="ca-field ca-settings-check">
              <input type="checkbox" checked={d.tailscale} onChange={(e) => set({ tailscale: e.target.checked })} />
              <span>Find my Macs on my tailnet</span>
              <small>
                {tailscaleWord(view)} Tailscale already knows your Macs and who they belong to; Codealong listens on this Mac's tailnet address only, and links only machines of yours.
              </small>
            </label>
            <label className="ca-field ca-settings-check">
              <input type="checkbox" checked={d.iroh} onChange={(e) => set({ iroh: e.target.checked })} />
              <span>Reach linked Macs anywhere</span>
              <small>
                {irohWord(view)} Through iroh: Macs dial each other by key and connect directly when they can; when they can't, number 0's relays pass the encrypted packets along without reading them.
              </small>
            </label>
            <label className="ca-field ca-settings-check">
              <input type="checkbox" checked={d.remote_control} onChange={(e) => set({ remote_control: e.target.checked })} />
              <span>Let my linked Macs act on sessions here</span>
              <small>
                They can send prompts, answer what agents wait on you to allow, and start sessions on this Mac. Every request is written to <code className="ca-ic">~/Library/Logs/Codealong/devices.log</code>.
                Reading and searching don't need this.
              </small>
            </label>
          </>
        )}
      </fieldset>
      {d.enabled && view && (
        <div className="ca-settings-cards">
          {me?.error && (
            <p className="ca-acct-note" role="alert">
              {me.error}
            </p>
          )}
          {view.asks.map((a) => (
            <div key={a.key} className="ca-ucard ca-acct ca-device-ask" role="alert">
              <div className="ca-ucard-head">
                <span className="ca-ucard-name ca-device-name">{a.name} wants to link</span>
                <span className="ca-device-code">{a.code}</span>
              </div>
              <p className="ca-acct-note">Link only if {a.name} shows the same code. Linked, each Mac shows the other's sessions and can read them.</p>
              <div className="ca-acct-actions">
                <Button size="sm" variant="primary" disabled={busy !== null} onClick={() => void run('answer', () => answerLink(a.key, true).then(() => `Linked with ${a.name}.`))}>
                  Link
                </Button>
                <Button size="sm" disabled={busy !== null} onClick={() => void run('answer', () => answerLink(a.key, false).then(() => null))}>
                  Not now
                </Button>
              </div>
            </div>
          ))}
          {view.devices.map((dev) => (
            <DeviceCard key={dev.key} dev={dev} now={now} busy={busy !== null} onUnlink={() => void run('unlink', () => unlinkDevice(dev.key).then(() => `Unlinked ${dev.name}.`))} />
          ))}
          {view.devices.length === 0 && view.asks.length === 0 && <p className="ca-empty">No Macs linked yet.</p>}
          {view.found.map((f) => (
            <div key={f.key} className="ca-ucard ca-acct">
              <div className="ca-ucard-head">
                <span className="ca-ucard-name ca-device-name">{f.name}</span>
                <span className="ca-ucard-plan">On your tailnet</span>
                {f.asking && <span className="ca-device-code">{f.code}</span>}
              </div>
              {f.asking ? (
                <p className="ca-acct-note">Waiting for you to link on {f.name}: it shows the same code.</p>
              ) : (
                <div className="ca-acct-actions">
                  <Button size="sm" icon="link" disabled={busy !== null} onClick={() => void run('link', () => linkDevice(f.key).then(() => null))}>
                    Link
                  </Button>
                  <span className="ca-acct-asof">Then say yes on {f.name}.</span>
                </div>
              )}
            </div>
          ))}
          {d.iroh && (
            <div className="ca-ucard ca-acct">
              <div className="ca-ucard-head">
                <span className="ca-ucard-name ca-device-name">A Mac that isn&apos;t on your tailnet</span>
              </div>
              <p className="ca-acct-note">Make a link code here and enter it on the other Mac, or enter one made there. A code works once, for ten minutes.</p>
              {code && (
                <div className="ca-device-linkcode">
                  <code className="ca-code" aria-label="Link code">
                    {code}
                  </code>
                  <Button size="sm" variant="ghost" icon="copy" aria-label="Copy the link code" onClick={() => void navigator.clipboard?.writeText(code)} />
                </div>
              )}
              <form
                className="ca-acct-actions"
                onSubmit={(e) => {
                  e.preventDefault();
                  const c = entered.trim();
                  if (c) void run('code', () => linkWithCode(c).then((who) => (setEntered(''), `Linked with ${who}.`)));
                }}
              >
                <Button size="sm" disabled={busy !== null} onClick={() => void run('make', () => deviceLinkCode().then((c) => (setCode(c), null)))}>
                  Make a link code
                </Button>
                <input className="ca-device-codein" value={entered} placeholder="Or enter one from another Mac" spellCheck={false} aria-label="Link code from another Mac" onChange={(e) => setEntered(e.target.value)} />
                <Button size="sm" type="submit" disabled={busy !== null || !entered.trim()}>
                  {busy === 'code' ? 'Linking…' : 'Link'}
                </Button>
              </form>
            </div>
          )}
          {note && (
            <p className={note.ok ? 'ca-acct-note' : 'ca-form-error'} role="status">
              {note.text}
            </p>
          )}
        </div>
      )}
    </>
  );
}

function DeviceCard({ dev, now, busy, onUnlink }: { dev: DeviceView; now: number; busy: boolean; onUnlink: () => void }) {
  const where = dev.online ? (dev.via === 'iroh' ? 'Connected through iroh' : 'Connected on your tailnet') : dev.lastSeen ? `Last seen ${agoPhrase(dev.lastSeen, now)}` : 'Not connected';
  return (
    <div className="ca-ucard ca-acct">
      <div className="ca-ucard-head">
        <span className={dev.online ? 'ca-device-dot is-on' : 'ca-device-dot'} aria-hidden="true" />
        <span className="ca-ucard-name ca-device-name">{dev.name}</span>
        <span className="ca-ucard-plan">{where}</span>
        <span className={['ca-tag', dev.remoteControl ? 'is-attn' : ''].join(' ')} title="Whether this Mac can send prompts and answer approvals there">
          {dev.remoteControl ? 'Takes remote control' : 'Read only'}
        </span>
      </div>
      {!dev.linkedBack && <p className="ca-acct-note">{dev.name} doesn't have this Mac linked any more. Unlink it here, then link again.</p>}
      <div className="ca-acct-actions">
        <Button size="sm" disabled={busy} onClick={onUnlink}>
          Unlink
        </Button>
        {dev.app && <span className="ca-acct-asof">Codealong {dev.app}</span>}
      </div>
    </div>
  );
}

function tailscaleWord(view: DevicesView | null): string {
  const me = view?.me;
  if (!me) return '';
  switch (me.tailscale) {
    case 'running':
      return me.address ? `On ${me.tailnet ?? 'your tailnet'}, listening at ${me.address}.` : `On ${me.tailnet ?? 'your tailnet'}.`;
    case 'missing':
      return 'Tailscale isn’t installed.';
    case 'stopped':
      return 'Tailscale isn’t connected.';
    default:
      return '';
  }
}

function irohWord(view: DevicesView | null): string {
  const s = view?.me.iroh ?? 'off';
  if (s === 'off') return '';
  if (s === 'online') return 'Reachable.';
  if (s === 'starting') return 'Starting…';
  if (s === 'offline') return 'Not reachable yet: no relay answered.';
  return `Didn’t start: ${s}.`;
}
