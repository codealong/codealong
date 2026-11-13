// Repos, worktrees and sessions as a picker's options (Picker.tsx), wherever one is picked: the
// quick panel's repo and worktree menus, and the schedule dialog's worktree and session fields. A
// worktree reads by name and branch with its status; a session by its title, where it works and
// its account (said last), with its status and how long since it last did anything. The schedule
// dialog lists worktrees in laneGroups' order, the seldom picked folded away; the quick panel
// lists repos (quick.ts `repoChoices`), then the worktrees of the one picked (`repoWorktrees`).
// Its repo menu also takes a folder's path, new or not on the board, as folderChoices offers it.

import { ago } from '../format';
import { laneWaits } from '../lanes';
import { laneBranch, NO_PROJECT, worktreeName, type FolderChoice, type LaneGroup, type RepoChoice } from '../quick';
import { REUSE_STATE } from '../reusable';
import type { Lane, Session } from '../types';
import type { PickerOption, PickerSection } from './Picker';
import { Icon, SessionGlyph, StatusGlyph } from './primitives';

/** A worktree, picked by `id` (its lane id in the quick panel, its folder in a schedule). */
export function laneOption(l: Lane, id: string = l.id): PickerOption {
  return {
    id,
    text: [l.callsign, l.branch ?? '', l.repo, l.project, l.origin === 'main' ? 'main checkout' : ''].join(' '),
    label: l.origin === 'main' ? `${l.callsign} (main checkout)` : l.callsign,
    detail: laneBranch(l),
    lead: <StatusGlyph status={l.glyph} size={10} title="" waiting={laneWaits(l)} />,
  };
}

/** A repo in the quick panel's repo menu, picked by `REPO` and its key (quick.ts `repoKey`), with
 *  where a session would start in it; named after its project too where the panel lists several,
 *  as laneGroups' headings are. */
export const REPO = 'repo:';
export function repoOption(r: RepoChoice, startsIn: string, withProject = false): PickerOption {
  return { id: `${REPO}${r.key}`, text: `${r.repo} ${r.project}`, label: withProject ? `${r.project} · ${r.repo}` : r.repo, detail: startsIn || undefined, lead: <Icon name="folder" size={12} /> };
}

/** The repo menu's repos, in `repoChoices`' order (the ones with a pinned worktree first,
 *  whatever their project). */
export function repoSections(repos: RepoChoice[], startsIn: (r: RepoChoice) => string): PickerSection[] {
  const several = new Set(repos.map((r) => r.project)).size > 1;
  return [{ key: 'repos', options: repos.map((r) => repoOption(r, startsIn(r), several)) }];
}

/** A worktree beside its repo, in the quick panel's worktree menu: by its name there (the main
 *  checkout is `main`), with what a reusable one holds and its branch. */
export function worktreeOption(l: Lane): PickerOption {
  return { ...laneOption(l), label: worktreeName(l), detail: [l.reuse ? REUSE_STATE[l.reuse] : null, laneBranch(l)].filter(Boolean).join(' · ') || undefined };
}

/** A worktree found by its name in the quick panel's repo menu, among every repo's: with its
 *  repo and its branch. */
export function namedWorktreeOption(l: Lane): PickerOption {
  return { ...laneOption(l), detail: [l.repo, laneBranch(l)].filter(Boolean).join(' · ') };
}

/** The worktree menu's New worktree: one its harness makes for the session (start.ts
 *  `newWorktreeDetail` says who, or why it can't). */
export const NEW_WORKTREE = 'worktree:new';
export function newWorktreeOption(detail: string, title: string, disabled: boolean): PickerOption {
  return { id: NEW_WORKTREE, text: 'new worktree', label: 'New worktree', detail, title, disabled, lead: <Icon name="plus" size={12} /> };
}

/** The schedule dialog's worktree menu's sections, from quick.ts `laneGroups`: a repo's worktrees other than its
 *  main checkout and reusable ones folded away under a heading of their own. */
export function laneSections(groups: LaneGroup[], id?: (l: Lane) => string): PickerSection[] {
  return groups.map((g) => ({
    key: g.more ? `more:${g.label}` : g.label,
    head: g.more ? `${g.label} · other worktrees` : g.label,
    options: g.lanes.map((l) => laneOption(l, id?.(l))),
    folded: g.more,
  }));
}

/** A chat of its own, in no project: Codealong's chats, apart from every worktree. */
export function chatOption(id: string): PickerOption {
  return { id, text: 'no project chat question', label: NO_PROJECT, detail: 'A chat of its own', lead: <Icon name="message-circle" size={12} /> };
}

/** The id of a folder row that adds the folder to `project` (to a project of its own with none). */
export const ADD_FOLDER = 'folder:add:';

/** The project an `ADD_FOLDER` row adds to, none for a project of its own; undefined for any
 *  other row. */
export function addsTo(id: string): string | null | undefined {
  return id.startsWith(ADD_FOLDER) ? id.slice(ADD_FOLDER.length) || null : undefined;
}

/** A folder's path typed in the worktree menu, as rows (quick.ts `folderChoices`): the worktree
 *  the board has there, adding it to a project, why it can't be added, and folders in it to go on
 *  typing from. Each row's text has what's typed, which the filter matches. */
export function folderOptions(choices: FolderChoice[], typed: string, lanes: readonly Lane[]): PickerOption[] {
  return choices.flatMap((c): PickerOption[] => {
    switch (c.kind) {
      case 'worktree': {
        const l = lanes.find((x) => x.id === c.lane);
        return l ? [{ ...laneOption(l), text: typed }] : [];
      }
      case 'add':
        return [{ id: `${ADD_FOLDER}${c.project ?? ''}`, text: typed, label: c.label, detail: c.detail, lead: <Icon name="folder-plus" size={12} /> }];
      case 'refused':
        return [{ id: 'folder:refused', text: typed, label: c.why, disabled: true, lead: <Icon name="folder" size={12} /> }];
      case 'into':
        return [{ id: `folder:into:${c.path}`, text: `${typed} ${c.path}`, label: c.label, detail: c.path, lead: <Icon name="folder" size={12} />, fill: `${c.path}/` }];
    }
  });
}

/** The worktree menu's way to a folder it doesn't list: picked, it puts `start` (quick.ts
 *  `folderStart`) in the filter, to type the folder's name after. */
export function anotherFolderOption(start: string): PickerOption {
  return { id: 'folder:another', text: 'another folder new project path', label: 'Another folder…', detail: 'New, or not listed here: type its path', lead: <Icon name="folder-plus" size={12} />, fill: start };
}

/** A session, with the worktree it works in. */
export function sessionOption(lane: Lane, s: Session, now: number): PickerOption {
  const where = lane.startsChats ? NO_PROJECT : `${lane.project} · ${lane.callsign}`;
  return {
    id: s.key,
    text: [s.title, lane.project, lane.repo, lane.callsign, lane.branch ?? '', s.account, lane.startsChats ? 'chat' : ''].join(' '),
    label: s.title,
    detail: `${where} · ${s.account}`,
    lead: <SessionGlyph s={s} size={10} title="" />,
    trail: (
      <span className="ca-picker-trail" title={`Last active ${new Date(s.lastActivity).toLocaleString()}`}>
        {s.archived ? 'Archived' : ago(s.lastActivity, now)}
      </span>
    ),
  };
}
