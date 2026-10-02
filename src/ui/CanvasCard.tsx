// A card on the canvas: the face every card has (card/CardFace.tsx), placed where you put it, and
// marked when it's selected, hovered, tied to what is, or faded by the filter.

import { memo } from 'react';
import { CARD_H, CARD_W, type RefItem } from '../canvas/model';
import { CardFace } from './card/CardFace';
import type { CardData } from './card/data';
import { cx } from './primitives';

interface CardProps {
  item: RefItem;
  data: CardData | null;
  /** cardSig of data; the card redraws only when it, the position or the selection change. */
  sig: string;
  now: number;
  selected: boolean;
  /** Ties the card to the others it belongs with while one of them is hovered or selected. */
  related: boolean;
  /** Doesn't match the window's filter. */
  faded: boolean;
  /** Under the pointer. */
  hovered: boolean;
}

function Card({ item, data, now, selected, related, faded, hovered }: CardProps) {
  const style = { transform: `translate(${item.x}px, ${item.y}px)`, width: CARD_W, height: CARD_H[item.ref] };
  return <CardFace item={item} data={data} now={now} style={style} className={cx(selected && 'is-selected', related && 'is-related', hovered && 'is-hovered', faded && 'is-faded')} />;
}

export const CanvasCard = memo(
  Card,
  (a, b) => a.sig === b.sig && a.item.x === b.item.x && a.item.y === b.item.y && a.selected === b.selected && a.related === b.related && a.faded === b.faded && a.hovered === b.hovered && a.item.title === b.item.title,
);
