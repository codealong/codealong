// Settings › Devices: Codealong on your other devices (src-tauri/src/peers.rs). Turning it on,
// how this device finds the others, the devices it's linked with and their colors, linking more
// (and getting the iPhone app, which a session started here installs with you),
// remote control, and where this machine stands for the automations your machines share
// (shared.rs). The switches are `[devices]` in config.toml; linking goes through the engine,
// which keeps the list.

import { useEffect, useState } from 'react';
import { below, showContextMenu } from '../contextMenu';
import { answerLink, askAgent, deviceLinkCode, getDevices, linkCodeQr, linkDevice, linkWithCode, onDevices, setDeviceColor, unlinkDevice } from '../api';
import { iphoneTask, syncProjectsTask } from '../agentTasks';
import { agoPhrase } from '../format';
import { COLOR_WORD } from '../todos';
import type { Config, DevicesCfg, DevicesView, DeviceView, LabelColor, Rank } from '../types';
import { DEVICES_OFF, LABEL_COLORS } from '../types';
import { Button, CheckBox, Seg } from './primitives';
import { errorText } from '../report';
import { platform } from '../platform';

/** Where a machine stands for the automations your machines share, in words. */
const RANKS: { id: Rank; label: string; says: string }[] = [
  { id: 'first', label: 'First choice', says: 'This machine runs them whenever it can.' },
  { id: 'backup', label: 'Backup', says: 'This machine runs them only while no first choice can: one that’s asleep, closed or off.' },
];

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
  const [qr, setQr] = useState<string | null>(null);
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
      setNote({ ok: false, text: errorText(e) });
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
          Codealong on each of your devices shows the others' worktrees and sessions here, reads their conversations and searches what was said in them. Devices talk to each other directly, over
          your tailnet or through iroh, encrypted end to end; nothing goes through a server of Codealong's.
        </p>
      </div>
      <fieldset className="ca-settings-fields" disabled={disabled}>
        <label className="ca-field ca-settings-check">
          <CheckBox checked={d.enabled} onChange={(on) => set({ enabled: on })} />
          <span>Show my other devices</span>
          <small>While this is off, this device talks to no other.</small>
        </label>
        {d.enabled && (
          <>
            <label className="ca-field">
              <span>This device's name</span>
              <input
                value={name}
                placeholder={me?.name ?? 'The computer’s name'}
                spellCheck={false}
                onChange={(e) => setName(e.target.value)}
                onBlur={() => name.trim() !== d.name && set({ name: name.trim() })}
                onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
              />
              <small>What your other devices call it. Empty uses the computer's own name.</small>
            </label>
            <label className="ca-field ca-settings-check">
              <CheckBox checked={d.tailscale} onChange={(on) => set({ tailscale: on })} />
              <span>Find my devices on my tailnet</span>
              <small>
                {tailscaleWord(view)} Tailscale already knows your devices and whose they are; Codealong listens on this device's tailnet address only, and links only devices of yours.
              </small>
            </label>
            <label className="ca-field ca-settings-check">
              <CheckBox checked={d.iroh} onChange={(on) => set({ iroh: on })} />
              <span>Reach linked devices anywhere</span>
              <small>
                {irohWord(view)} Through iroh: devices dial each other by key and connect directly when they can; when they can't, number 0's relays pass the encrypted packets along without reading them.
              </small>
            </label>
            <label className="ca-field ca-settings-check">
              <CheckBox checked={d.remote_control} onChange={(on) => set({ remote_control: on })} />
              <span>Let my linked devices act on sessions here</span>
              <small>
                They can send prompts, answer what agents wait on you to allow, and start sessions on this device. Every request is written to <code className="ca-ic">~/Library/Logs/Codealong/devices.log</code>.
                Reading and searching don't need this.
              </small>
            </label>
            <label className="ca-field ca-settings-check">
              <CheckBox checked={d.in_bar} onChange={(on) => set({ in_bar: on })} />
              <span>Show their sessions in the bar and the notch</span>
              <small>What waits on you on your other devices shows in the bar, the notch and Waiting on you too, and plays its sound here, while they're connected.</small>
            </label>
            <div className="ca-field">
              <span>For automations my machines share</span>
              <Seg className="ca-settings-theme" label="For automations my machines share" options={RANKS} value={d.automations} onPick={(id) => id !== d.automations && set({ automations: id })} />
              <small>
                One of your machines runs each of these rather than all of them: reviewing a pull request that asks for your review, among the machines where automatic reviews are on, and a schedule that runs on one of your machines, after the one it prefers.{' '}
                {RANKS.find((r) => r.id === d.automations)?.says} Machines with the same setting go in an order they all agree on.
              </small>
            </div>
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
              <p className="ca-acct-note">Link only if {a.name} shows the same code. Linked, each device shows the other's sessions and can read them.</p>
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
            <DeviceCard
              key={dev.key}
              dev={dev}
              now={now}
              busy={busy !== null}
              // The engine refuses a color it can't save, and says why here.
              onColor={(color) => void run('color', () => setDeviceColor(dev.key, color).then(() => null))}
              onUnlink={() => void run('unlink', () => unlinkDevice(dev.key).then(() => `Unlinked ${dev.name}.`))}
            />
          ))}
          {view.phones.map((p) => (
            <PhoneCard key={p.key} dev={p} now={now} busy={busy !== null} remoteControl={me?.remoteControl ?? false} onUnlink={() => void run('unlink', () => unlinkDevice(p.key).then(() => `Unlinked ${p.name}.`))} />
          ))}
          {view.devices.length === 0 && view.phones.length === 0 && view.asks.length === 0 && <p className="ca-empty">No devices linked yet.</p>}
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
                <span className="ca-ucard-name ca-device-name">Your phone, or a device that isn&apos;t on your tailnet</span>
              </div>
              <p className="ca-acct-note">
                Make a link code here and scan it with your phone&apos;s camera, or enter it on the other device; or enter one made there. A code works once, for ten minutes.
              </p>
              {qr && <img className="ca-device-qr" src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(qr)}`} alt="The link code as a QR code, for your phone's camera" />}
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
                <Button
                  size="sm"
                  disabled={busy !== null}
                  onClick={() =>
                    void run('make', async () => {
                      const c = await deviceLinkCode();
                      setCode(c);
                      // The code shows in words either way; the picture is for a phone's camera.
                      setQr(await linkCodeQr(c).catch(() => null));
                      return null;
                    })
                  }
                >
                  Make a link code
                </Button>
                <input className="ca-device-codein" value={entered} placeholder="Or enter one from another device" spellCheck={false} aria-label="Link code from another device" onChange={(e) => setEntered(e.target.value)} />
                <Button size="sm" type="submit" disabled={busy !== null || !entered.trim()}>
                  {busy === 'code' ? 'Linking…' : 'Link'}
                </Button>
              </form>
              {/* Building it takes Xcode, so a Mac. The panel opens on a chat of its own, which
                  finds or clones the app's repo itself. */}
              {platform() === 'mac' && (
                <div className="ca-acct-actions">
                  <Button size="sm" icon="device-mobile" onClick={() => void askAgent(iphoneTask())}>
                    Get the iPhone app…
                  </Button>
                  <span className="ca-acct-asof">It isn’t on the App Store yet: an agent builds it and installs it on your phone with you.</span>
                </div>
              )}
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

function DeviceCard({ dev, now, busy, onColor, onUnlink }: { dev: DeviceView; now: number; busy: boolean; onColor: (color: LabelColor) => void; onUnlink: () => void }) {
  const where = dev.online ? (dev.via === 'iroh' ? 'Connected through iroh' : 'Connected on your tailnet') : dev.lastSeen ? `Last seen ${agoPhrase(dev.lastSeen, now)}` : 'Not connected';
  // Its projects come with its worktrees, so there's nothing to match until it has sent them.
  const projects = dev.projects ?? [];
  return (
    <div className="ca-ucard ca-acct">
      <div className="ca-ucard-head">
        <span className={dev.online ? 'ca-device-dot is-on' : 'ca-device-dot'} aria-hidden="true" />
        <button
          type="button"
          className="ca-devcolor"
          title={`${COLOR_WORD[dev.color]}: the color of ${dev.name}'s badge. Click to change it.`}
          aria-label={`${dev.name}'s color, ${COLOR_WORD[dev.color]}`}
          onClick={(e) => void showContextMenu(LABEL_COLORS.map((c: LabelColor) => ({ label: COLOR_WORD[c], checked: dev.color === c, run: () => onColor(c) })), below(e.currentTarget))}
        >
          <i className={`ca-labeldot is-${dev.color}`} aria-hidden="true" />
        </button>
        <span className="ca-ucard-name ca-device-name">{dev.name}</span>
        <span className="ca-ucard-plan">{where}</span>
        <span className="ca-acct-tags">
          {dev.automations && (
            <span className="ca-tag" title="Where it stands for the automations your machines share">
              {RANKS.find((r) => r.id === dev.automations)?.label}
            </span>
          )}
          <span className={['ca-tag', dev.remoteControl ? 'is-attn' : ''].join(' ')} title="Whether this device can send prompts and answer approvals there">
            {dev.remoteControl ? 'Takes remote control' : 'Read only'}
          </span>
        </span>
      </div>
      {!dev.linkedBack && <p className="ca-acct-note">{dev.name} doesn't have this device linked any more. Unlink it here, then link again.</p>}
      <div className="ca-acct-actions">
        <Button
          size="sm"
          icon="folder"
          disabled={projects.length === 0}
          title={projects.length > 0 ? `Opens the quick panel with a prompt for an agent to make this machine’s projects match ${dev.name}’s, asking you before it changes anything` : `Nothing from ${dev.name} yet: its projects come with its worktrees once it’s connected.`}
          onClick={() => void askAgent(syncProjectsTask(dev.name, projects))}
        >
          Sync projects…
        </Button>
        <Button size="sm" disabled={busy} onClick={onUnlink}>
          Unlink
        </Button>
        {dev.app && <span className="ca-acct-asof">Codealong {dev.app}</span>}
      </div>
    </div>
  );
}

/** A linked phone or tablet: it only watches, so it has no color, worktrees or remote control of its own. */
function PhoneCard({ dev, now, busy, remoteControl, onUnlink }: { dev: DeviceView; now: number; busy: boolean; remoteControl: boolean; onUnlink: () => void }) {
  const where = dev.online ? 'Connected' : dev.lastSeen ? `Last seen ${agoPhrase(dev.lastSeen, now)}` : 'Not connected';
  return (
    <div className="ca-ucard ca-acct">
      <div className="ca-ucard-head">
        <span className={dev.online ? 'ca-device-dot is-on' : 'ca-device-dot'} aria-hidden="true" />
        <span className="ca-ucard-name ca-device-name">{dev.name}</span>
        <span className="ca-ucard-plan">
          {dev.kind === 'tablet' ? 'Tablet' : 'Phone'} · {where}
        </span>
      </div>
      <p className="ca-acct-note">
        {remoteControl
          ? `It sees this device's sessions and, with remote control on, can send prompts and answer approvals here.`
          : `It sees this device's sessions and reads them. Turn on remote control above to let it send prompts and answer approvals.`}
      </p>
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
