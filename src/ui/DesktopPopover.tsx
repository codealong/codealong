// The popover of a card pinned to the desktop: the one its Canvas card opens (card/CardPopover.tsx),
// in a panel of its own beside the card, since the card's window is only as big as the card
// (cardpop.rs places it and closes it). The engine sends the card's feed as it opens; the page lays
// it out and says how tall it is, and only then does the panel show. A session line in it opens
// that session in the peek by the card, and its right-click menu is the session's, with Pin to
// desktop: the way to put the sessions a session came from or started beside it.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { cardPopoverFit, cardPopoverReady, cardPopoverSession, onCardPopover, onCardPopoverHidden, openUrl, showMain, type CardPopoverOpen } from '../api';
import { showContextMenu } from '../contextMenu';
import { useEditorIcons } from '../editors';
import { sessionHomes } from '../lanes';
import * as menus from '../menus';
import { openWhere, where } from '../open';
import { errorText, type Report } from '../report';
import type { Label, Lane, Session } from '../types';
import { CardDetail, CardPopover } from './card/CardPopover';
import { cardData } from './card/data';
import { useTheme } from './ThemeSwitch';
import { useNow } from './useNow';

export function DesktopPopover() {
  const [open, setOpen] = useState<CardPopoverOpen | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const now = useNow(30_000);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const stops = [
      onCardPopover((o) => {
        setFailed(null);
        setOpen(o);
      }),
      onCardPopoverHidden(() => setOpen(null)),
    ];
    return () => stops.forEach((stop) => stop());
  }, []);
  const feed = open?.feed ?? null;
  useTheme(feed?.theme ?? 'system');
  const icons = useEditorIcons(feed ?? undefined);
  const pinned = menus.useDesktopPins();

  const item = useMemo(() => ({ ref: feed?.pin.ref ?? 'session', key: feed?.pin.key ?? '', title: feed?.title ?? undefined }), [feed]);
  const homes = useMemo(() => sessionHomes(feed?.lanes ?? []), [feed]);
  const data = useMemo(() => {
    if (!feed) return null;
    const entities = { lanes: new Map(feed.lanes.map((l) => [l.id, l])), sessions: homes, todos: new Map(feed.todos.map((t) => [t.id, t])), prs: new Map() };
    const labels = new Map<string, Label>(feed.labels.map((l) => [l.id, l]));
    return cardData(entities, { kind: 'ref', x: 0, y: 0, ...item }, labels);
  }, [feed, homes, item]);

  // Once laid out for an open, the panel shows at this height; after that it follows the page:
  // after every render, and on resizes renders don't cause (fonts loading).
  const answered = useRef<{ id: number; height: number } | null>(null);
  const openNow = useRef(open);
  openNow.current = open;
  const report = useCallback(() => {
    const el = root.current;
    const o = openNow.current;
    if (!o || !el) return;
    const height = Math.ceil(el.getBoundingClientRect().height);
    const last = answered.current;
    if (last?.id !== o.id) void cardPopoverReady(o.id, height);
    else if (last.height !== height) void cardPopoverFit(height);
    answered.current = { id: o.id, height };
  }, []);
  useLayoutEffect(report);
  const shown = !!open;
  useEffect(() => {
    if (!root.current) return;
    const observer = new ResizeObserver(report);
    observer.observe(root.current);
    return () => observer.disconnect();
  }, [report, shown]);

  if (!open || !feed) return null;
  const said: Report = { error: setFailed, info: setFailed };
  const sessionMenu = (lane: Lane, s: Session, at: { x: number; y: number }) =>
    void showContextMenu(
      menus.sessionMenu(lane, s, {
        report: said,
        open: (l, target) => openWhere(where(l, target), { report: said, from: 'panel' }),
        terminal: feed.terminal,
        reveal: () => void showMain({ kind: 'session', laneId: lane.id, key: s.key }),
        reply: () => void cardPopoverSession(s.key).catch((e: unknown) => setFailed(errorText(e))),
        editors: { apps: feed, icons },
        desktop: pinned,
      }),
      at,
    ).catch((e: unknown) => setFailed(errorText(e)));

  return (
    <div ref={root} className={`ca-deskpop is-${open.side}`} style={{ maxHeight: open.room }}>
      <CardPopover>
        <CardDetail
          item={item}
          data={data}
          now={now}
          terminal={feed.terminal}
          todos={feed.todos}
          place="desktop"
          homes={homes}
          onOpenPr={(pr) => openUrl(pr.url).catch((e: unknown) => setFailed(errorText(e)))}
          onSession={(_lane, s) => void cardPopoverSession(s.key).catch((e: unknown) => setFailed(errorText(e)))}
          onSessionMenu={sessionMenu}
        />
        {failed && (
          <p className="ca-deskpop-note" role="alert">
            {failed}
          </p>
        )}
      </CardPopover>
    </div>
  );
}
