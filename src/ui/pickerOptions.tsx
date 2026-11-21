// Repos, worktrees and sessions as a picker's options (Picker.tsx), wherever one is picked: the
// quick panel's repo and worktree menus, and the schedule dialog's worktree and session fields. A
// worktree reads by name and branch with its status; a session by its title, where it works and
// its account (said last), with its status and how long since it last did anything. The schedule
// dialog lists worktrees in laneGroups' order, the seldom picked folded away; the quick panel
// lists repos (quick.ts `repoChoices`), then the worktrees of the one picked (`repoWorktrees`).
// Its repo menu also takes a folder's path, new or not on the board, as folderChoices offers it.
// The panel's project and machine menus are here too (`projectMenu`, `machineMenu`).

import { ago } from '../format';
import { laneWaits } from '../lanes';
import { shortOf } from '../devices';
import { ALL_PROJECTS, CHATS, GENERAL_CHAT, inProject, laneBranch, macUnavailable, projectLabel, shortcutLabel, worktreeCount, worktreeName, type FolderChoice, type LaneGroup, type RepoChoice, type RepoWorktrees } from '../quick';
import { REUSE_STATE } from '../reusable';
import type { DevicesView, DeviceView, Lane, Load, ProjectSummary, Session } from '../types';
import { LoadTag } from './LoadTag';
import type { PickerOption, PickerSection } from './Picker';
import { Icon, SessionGlyph, StatusGlyph } from './primitives';
import { ProjectIcon } from './ProjectSaid';
import { projectKey } from './ProjectSwitcher';
import { scopeWords } from '../orchestrators';

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
/** The quick panel's worktree menu, of the repo a session starts in (quick.ts `repoWorktrees`): its
 *  main checkout first, then the line that makes a new worktree where one can be asked for
 *  (`newWorktree`), then the ones ready for new work and the pinned; the ones that hold other
 *  work are folded away. */
export function worktreeMenu(inRepo: RepoWorktrees, newWorktree: PickerOption | null): PickerSection[] {
  return [
    {
      key: 'new-work',
      options: [...(inRepo.main ? [worktreeOption(inRepo.main)] : []), ...(newWorktree ? [newWorktree] : []), ...[...inRepo.ready, ...inRepo.pinned].map(worktreeOption)],
    },
    { key: 'under-way', head: 'Work under way', folded: true, options: inRepo.underWay.map(worktreeOption) },
    { key: 'others', head: 'Other worktrees', folded: true, options: inRepo.others.map(worktreeOption) },
  ];
}

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

/** A general chat: a question for none of your repos, in a folder of its own among Codealong's
 *  chats, apart from every worktree. */
export function chatOption(id: string): PickerOption {
  return { id, text: 'general chat question no project no repo', label: GENERAL_CHAT, detail: 'In no repo: a folder of its own', lead: <Icon name="message-circle" size={12} /> };
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
  // An orchestrator is in Codealong's chats, and called Orchestrator: it's for what it keeps watch over.
  const scope = s.orchestrator ? scopeWords(s.orchestrator.project) : null;
  const where = scope ?? (lane.startsChats ? GENERAL_CHAT : `${lane.project} · ${lane.callsign}`);
  return {
    id: s.key,
    text: [s.title, scope ?? '', lane.project, lane.repo, lane.callsign, lane.branch ?? '', s.account, lane.startsChats ? 'chat' : ''].join(' '),
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

/** What waits on you in a project, on its line of the project menu. */
function ProjectCounts({ needs, unread }: { needs: number; unread: number }) {
  return (
    <span className="ca-quick-counts">
      {needs > 0 && (
        <span className="ca-projswitch-action is-needs" title={`${needs} need you`}>
          <StatusGlyph status="needs" size={10} title="" />
          {needs}
        </span>
      )}
      {unread > 0 && (
        <span className="ca-projswitch-action is-unread" title={`${unread} finished`}>
          <StatusGlyph status="unread" size={10} title="" />
          {unread}
        </span>
      )}
    </span>
  );
}

/** The quick panel's project menu, as in the main window's switcher: the projects, then All
 *  projects set apart, with General chat, for a question that belongs to none of them. Each says
 *  what waits on you there (`counts`) and the key that goes to it, General chat's being its
 *  shortcut. */
export function projectMenu(projects: ProjectSummary[], counts: Record<string, { needs: number; unread: number }>, chatShortcut: string | null | undefined): PickerSection[] {
  const projectOption = (name: string, icon?: string | null): PickerOption => ({
    id: name,
    text: name === CHATS ? `${GENERAL_CHAT} question no project no repo` : name,
    label: projectLabel(name),
    detail: name === CHATS ? 'A question for none of your repos' : undefined,
    lead: name === ALL_PROJECTS ? <Icon name="stack" size={14} /> : name === CHATS ? <Icon name="message-circle" size={14} /> : <ProjectIcon src={icon} size={14} fallback />,
    trail: (
      <>
        <ProjectCounts needs={counts[name]?.needs ?? 0} unread={counts[name]?.unread ?? 0} />
        <span className="ca-projswitch-key">{name === CHATS ? shortcutLabel(chatShortcut) : projectKey(name, projects)}</span>
      </>
    ),
  });
  return [
    { key: 'projects', options: projects.map((p) => projectOption(p.name, p.icon)) },
    { key: 'all', head: <span className="ca-picker-sep" />, options: [projectOption(ALL_PROJECTS), projectOption(CHATS)] },
  ];
}

/** The machine menu's id for this machine; the others go by their short ids, which are hex. */
export const HERE = 'here';

/** The quick panel's machine menu: this machine (its worktrees `lanes`, how busy it is `load`),
 *  then your other machines, each with how many worktrees it has in the project picked, or why
 *  a session can't start there now. */
export function machineMenu(devices: DevicesView, macs: DeviceView[], lanes: Lane[], load: Load | null | undefined, current: string, now: number): PickerSection[] {
  // On General chat, each machine's worktrees: its chats are in none.
  const scope = current === CHATS ? ALL_PROJECTS : current;
  const count = (short: string | null) => worktreeCount((short ? devices.lanes.filter((l) => shortOf(l.id) === short) : lanes).filter((l) => !l.startsChats && inProject(l, scope)).length, scope);
  const here = devices.me.name || 'This machine';
  return [
    {
      key: 'macs',
      options: [
        { id: HERE, text: `${here} this machine this mac`, label: here, detail: `This machine · ${count(null)}`, lead: <Icon name="device-desktop" size={14} />, trail: load ? <LoadTag load={load} name={here} plain /> : undefined },
        ...macs.map((d) => {
          const { away, off, title } = macUnavailable(d, now);
          return {
            id: d.short,
            text: d.name,
            label: d.name,
            detail: away ?? off ?? count(d.short),
            lead: <i className={`ca-labeldot is-${d.color}`} aria-hidden="true" />,
            trail: d.load && !away ? <LoadTag load={d.load} name={d.name} plain /> : undefined,
            disabled: !!(away || off),
            title,
          };
        }),
      ],
    },
  ];
}
