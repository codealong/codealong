// Two-stage deterministic damped force-directed layout + overlap-free rectangular packing for
// Working Set Canvas (workingSet.ts). Stage 1 settles nodes inside each cluster around any
// user-pinned anchors; Stage 2 packs cluster bounding boxes and standalone nodes cleanly with
// soft cross-cluster edge attraction and a widescreen-friendly aspect bias.

import type { NodeBox, WorkingCluster, WorkingEdge, WorkingNode } from './workingSet';

export const WS_CARD_W = 296;
export const WS_CARD_H = 120;
export const WS_HUB_W = 256;
export const WS_HUB_H = 80;

const CARD_GAP_X = 20;
const CARD_GAP_Y = 18;
const CLUSTER_PAD = 18;
const CLUSTER_HEAD = 40;
const CLUSTER_GAP = 32;

interface Body {
  id: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  w: number;
  h: number;
  pinned: boolean;
  rank: number;
}

export function hashStr(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

function nodeSize(n: WorkingNode): { w: number; h: number } {
  return n.kind === 'schedule' ? { w: WS_HUB_W, h: WS_HUB_H } : { w: WS_CARD_W, h: WS_CARD_H };
}

/** Resolves rectangular AABB overlaps between bodies, keeping user-pinned anchors fixed. */
function resolveRectCollisions(bodies: Body[], gapX: number, gapY: number, strength = 0.5) {
  for (let i = 0; i < bodies.length; i++) {
    const a = bodies[i];
    for (let j = i + 1; j < bodies.length; j++) {
      const b = bodies[j];
      const dx = b.x + b.w / 2 - (a.x + a.w / 2);
      const dy = b.y + b.h / 2 - (a.y + a.h / 2);
      const ox = (a.w + b.w) / 2 + gapX - Math.abs(dx);
      const oy = (a.h + b.h) / 2 + gapY - Math.abs(dy);
      if (ox <= 0 || oy <= 0) continue;

      let sx = 0;
      let sy = 0;
      if (ox < oy) sx = (dx >= 0 ? 1 : -1) * ox * strength;
      else sy = (dy >= 0 ? 1 : -1) * oy * strength;

      if (a.pinned && b.pinned) continue;
      if (a.pinned) {
        b.x += sx * 2;
        b.y += sy * 2;
      } else if (b.pinned) {
        a.x -= sx * 2;
        a.y -= sy * 2;
      } else {
        a.x -= sx;
        a.y -= sy;
        b.x += sx;
        b.y += sy;
      }
    }
  }
}

export function layoutWorkingSet(
  nodes: WorkingNode[],
  edges: WorkingEdge[],
  clusters: WorkingCluster[],
  pinnedPos: Record<string, { x: number; y: number }>,
): Map<string, NodeBox> {
  const boxes = new Map<string, NodeBox>();
  if (nodes.length === 0) return boxes;

  const nodeById = new Map(nodes.map((n) => [n.id, n]));
  const clusterOf = new Map<string, WorkingCluster>();
  for (const cl of clusters) for (const id of cl.nodeIds) clusterOf.set(id, cl);

  // Stage 1: Settle each cluster internally with damped springs + box collision.
  const localBoxes = new Map<string, NodeBox>();
  const clusterPinned = new Set<string>();

  for (const cl of clusters) {
    const memberSet = new Set(cl.nodeIds);
    if (cl.nodeIds.some((id) => !!pinnedPos[id])) clusterPinned.add(cl.id);

    const cols = Math.min(3, Math.max(2, Math.ceil(Math.sqrt(cl.nodeIds.length * 1.4))));
    const bodies: Body[] = cl.nodeIds.map((id, idx) => {
      const n = nodeById.get(id)!;
      const { w, h } = nodeSize(n);
      const pin = pinnedPos[id];
      const jitter = ((hashStr(id) % 19) - 9) * 1.2;
      return {
        id,
        x: pin ? pin.x : (idx % cols) * (WS_CARD_W + CARD_GAP_X) + jitter,
        y: pin ? pin.y : Math.floor(idx / cols) * (WS_CARD_H + CARD_GAP_Y) + jitter * 0.5,
        vx: 0,
        vy: 0,
        w,
        h,
        pinned: !!pin,
        rank: n.action.rank,
      };
    });
    const bodyById = new Map(bodies.map((b) => [b.id, b]));
    const innerEdges = edges.filter((e) => memberSet.has(e.from) && memberSet.has(e.to));

    const pinnedBodies = bodies.filter((b) => b.pinned);
    const cxTarget = pinnedBodies.length ? pinnedBodies.reduce((s, b) => s + b.x + b.w / 2, 0) / pinnedBodies.length : ((cols - 1) * (WS_CARD_W + CARD_GAP_X)) / 2;
    const cyTarget = pinnedBodies.length ? pinnedBodies.reduce((s, b) => s + b.y + b.h / 2, 0) / pinnedBodies.length : WS_CARD_H;

    for (let tick = 0; tick < 85; tick++) {
      for (const e of innerEdges) {
        const a = bodyById.get(e.from);
        const b = bodyById.get(e.to);
        if (!a || !b) continue;
        const dx = b.x + b.w / 2 - (a.x + a.w / 2);
        const dy = b.y + b.h / 2 - (a.y + a.h / 2);
        const dist = Math.hypot(dx, dy) || 1;
        const ideal = Math.hypot((a.w + b.w) * 0.36 + CARD_GAP_X, (a.h + b.h) * 0.36 + CARD_GAP_Y);
        const f = ((dist - ideal) / dist) * 0.045 * e.weight;
        if (!a.pinned) {
          a.vx += dx * f;
          a.vy += dy * f;
        }
        if (!b.pinned) {
          b.vx -= dx * f;
          b.vy -= dy * f;
        }
      }
      for (const b of bodies) {
        if (b.pinned) continue;
        b.vx += (cxTarget - (b.x + b.w / 2)) * 0.014;
        b.vy += (cyTarget + (b.rank - 1.5) * 18 - (b.y + b.h / 2)) * 0.018;
        b.vx *= 0.8;
        b.vy *= 0.8;
        b.x += b.vx;
        b.y += b.vy;
      }
      resolveRectCollisions(bodies, CARD_GAP_X, CARD_GAP_Y, 0.52);
    }
    for (let p = 0; p < 12; p++) resolveRectCollisions(bodies, CARD_GAP_X, CARD_GAP_Y, 0.55);

    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const b of bodies) {
      b.x = Math.round(b.x);
      b.y = Math.round(b.y);
      minX = Math.min(minX, b.x);
      minY = Math.min(minY, b.y);
      maxX = Math.max(maxX, b.x + b.w);
      maxY = Math.max(maxY, b.y + b.h);
      localBoxes.set(b.id, { x: b.x, y: b.y, w: b.w, h: b.h });
    }
    cl.x = minX - CLUSTER_PAD;
    cl.y = minY - CLUSTER_HEAD;
    cl.w = maxX - minX + CLUSTER_PAD * 2;
    cl.h = maxY - minY + CLUSTER_HEAD + CLUSTER_PAD;
  }

  // Stage 2: Macro layout of clusters + standalone nodes with cross-cluster spring attraction.
  interface MacroItem {
    kind: 'cluster' | 'single';
    id: string;
    body: Body;
    cluster?: WorkingCluster;
    node?: WorkingNode;
    originX: number;
    originY: number;
  }

  const macros: MacroItem[] = [];
  const macroOfNode = new Map<string, string>();
  const orderedClusters = [...clusters].sort((a, b) => b.urgentCount - a.urgentCount || b.actionCount - a.actionCount || b.nodeIds.length - a.nodeIds.length || a.id.localeCompare(b.id));
  const singles = nodes.filter((n) => !clusterOf.has(n.id)).sort((a, b) => a.action.rank - b.action.rank || a.id.localeCompare(b.id));

  const macroCols = Math.max(2, Math.min(3, Math.ceil(Math.sqrt((orderedClusters.length + singles.length) * 1.15))));
  let cursor = 0;
  for (const cl of orderedClusters) {
    const pinned = clusterPinned.has(cl.id);
    const x = pinned ? cl.x : (cursor % macroCols) * 520;
    const y = pinned ? cl.y : Math.floor(cursor / macroCols) * 280;
    cursor++;
    const body: Body = { id: cl.id, x, y, vx: 0, vy: 0, w: cl.w, h: cl.h, pinned, rank: cl.urgentCount > 0 ? 0 : cl.actionCount > 0 ? 1 : 3 };
    macros.push({ kind: 'cluster', id: cl.id, body, cluster: cl, originX: cl.x, originY: cl.y });
    for (const nid of cl.nodeIds) macroOfNode.set(nid, cl.id);
  }

  for (const n of singles) {
    const { w, h } = nodeSize(n);
    const pin = pinnedPos[n.id];
    const x = pin ? pin.x : (cursor % macroCols) * 320 + 20;
    const y = pin ? pin.y : Math.floor(cursor / macroCols) * 170 + 20;
    cursor++;
    const body: Body = { id: n.id, x, y, vx: 0, vy: 0, w, h, pinned: !!pin, rank: n.action.rank };
    macros.push({ kind: 'single', id: n.id, body, node: n, originX: x, originY: y });
    macroOfNode.set(n.id, n.id);
  }

  const macroById = new Map(macros.map((m) => [m.id, m]));
  const macroBodies = macros.map((m) => m.body);
  const macroEdges: { a: Body; b: Body; weight: number }[] = [];
  for (const e of edges) {
    const ma = macroOfNode.get(e.from);
    const mb = macroOfNode.get(e.to);
    if (!ma || !mb || ma === mb) continue;
    macroEdges.push({ a: macroById.get(ma)!.body, b: macroById.get(mb)!.body, weight: e.weight });
  }

  for (let tick = 0; tick < 75; tick++) {
    for (const me of macroEdges) {
      const dx = me.b.x + me.b.w / 2 - (me.a.x + me.a.w / 2);
      const dy = me.b.y + me.b.h / 2 - (me.a.y + me.a.h / 2);
      const dist = Math.hypot(dx, dy) || 1;
      const ideal = Math.hypot((me.a.w + me.b.w) * 0.42 + CLUSTER_GAP, (me.a.h + me.b.h) * 0.42 + CLUSTER_GAP);
      const f = ((dist - ideal) / dist) * 0.04 * me.weight;
      if (!me.a.pinned) {
        me.a.vx += dx * f;
        me.a.vy += dy * f;
      }
      if (!me.b.pinned) {
        me.b.vx -= dx * f;
        me.b.vy -= dy * f;
      }
    }
    for (const b of macroBodies) {
      if (b.pinned) continue;
      b.vx += -(b.x + b.w / 2) * 0.01;
      b.vy += (b.rank * 35 - (b.y + b.h / 2)) * 0.016;
      b.vx *= 0.78;
      b.vy *= 0.78;
      b.x += b.vx;
      b.y += b.vy;
    }
    resolveRectCollisions(macroBodies, CLUSTER_GAP, CLUSTER_GAP, 0.52);
  }
  for (let p = 0; p < 16; p++) resolveRectCollisions(macroBodies, CLUSTER_GAP, CLUSTER_GAP, 0.58);

  for (const m of macros) {
    const fx = Math.round(m.body.x);
    const fy = Math.round(m.body.y);
    if (m.kind === 'cluster' && m.cluster) {
      const dx = fx - m.originX;
      const dy = fy - m.originY;
      m.cluster.x = fx;
      m.cluster.y = fy;
      for (const nid of m.cluster.nodeIds) {
        const lb = localBoxes.get(nid)!;
        boxes.set(nid, { x: lb.x + dx, y: lb.y + dy, w: lb.w, h: lb.h });
      }
    } else if (m.node) {
      boxes.set(m.id, { x: fx, y: fy, w: m.body.w, h: m.body.h });
    }
  }

  return boxes;
}
