// A worktree's or a PR's project, said quietly before its name where the window lists another
// project or all of them. Under All projects, sessions from every project sit side by side, and a
// worktree's name alone (`fix-login`, `app2`) doesn't say whose work it is. The project goes
// first, as the first step of project / repo / worktree, in the place's quieter color, so a
// window of one project looks as it did.
//
// A window says which project it lists (`ProjectShown`): the main window, the quick panel and the
// floating bar their picked one, the waiting window and a kept session All projects, the peek
// the bar's. A row that has already said a project (a todo's names its own) says it again for
// what's inside it, so its worktrees don't repeat it. Where nothing says one (a card pinned to
// the desktop), no project is said.

import { createContext, useContext, useState, type ReactNode } from 'react';
import { Icon } from './primitives';

export const ProjectShown = createContext<string | null>(null);

const EMPTY_ICONS: ReadonlyMap<string, string> = new Map();
/** Project name → cached GitHub org avatar `data:image/…` URL, for All projects badges. */
export const ProjectIcons = createContext<ReadonlyMap<string, string>>(EMPTY_ICONS);

/** Builds the lookup map for `ProjectIcons.Provider` from the board's projects. */
export function projectIconMap(projects: readonly { name: string; icon?: string | null }[] | undefined): ReadonlyMap<string, string> {
  if (!projects || projects.length === 0) return EMPTY_ICONS;
  const map = new Map<string, string>();
  for (const p of projects) {
    if (p.icon) map.set(p.name, p.icon);
  }
  return map.size > 0 ? map : EMPTY_ICONS;
}

/** A project's GitHub org avatar, falling back to the folder icon (`fallback={true}`), a custom
 *  element (such as Codealong's `Mark`), or nothing when absent or broken. */
export function ProjectIcon({ src, size, fallback = false }: { src?: string | null; size: number; fallback?: boolean | ReactNode }) {
  const [failed, setFailed] = useState<string | null>(null);
  if (src && failed !== src) {
    return (
      <img
        className="ca-projicon"
        src={src}
        width={size}
        height={size}
        alt=""
        draggable={false}
        onError={() => setFailed(src)}
      />
    );
  }
  if (fallback === true) return <Icon name="folder" size={size} />;
  return fallback || null;
}

/** What's inside a row that said `project` already (a todo's own, a PR's repo chip) says it no
 *  more; a place in another project still says its own. */
export function ProjectSaidHere({ project, children }: { project: string | null | undefined; children: ReactNode }) {
  const shown = useContext(ProjectShown);
  return <ProjectShown.Provider value={shown != null && project ? project : shown}>{children}</ProjectShown.Provider>;
}

/** The project to say before a place in `project` where the window lists `shown`: none where the
 *  window lists no project or that one, or where a name said beside it already is the project's
 *  (a repo named as its project is). */
export function projectSaid(project: string | null | undefined, shown: string | null, beside: (string | null | undefined)[] = []): string | null {
  if (!project || shown == null || project === shown) return null;
  const same = project.toLowerCase();
  return beside.some((name) => name?.toLowerCase() === same) ? null : project;
}

/** `projectSaid` for the window this is in. */
export function useProjectSaid(project: string | null | undefined, beside: (string | null | undefined)[] = []): string | null {
  return projectSaid(project, useContext(ProjectShown), beside);
}

/** A place said in words (a tooltip, a menu item), after its project where `projectSaid` says
 *  it: `Northwind / fix-login`. */
export function placeWords(project: string | null | undefined, place: string, shown: string | null): string {
  const said = projectSaid(project, shown, [place]);
  return said ? `${said} / ${place}` : place;
}

export type PlaceWords = (project: string | null | undefined, place: string) => string;

/** `placeWords` for the window this is in. */
export function usePlaceWords(): PlaceWords {
  const shown = useContext(ProjectShown);
  return (project, place) => placeWords(project, place, shown);
}

/** A todo's project as a tag of its own, where the window lists another or all of them: a todo
 *  needn't be in any worktree, so it has no place's name to say it before. */
export function ProjectTag({ project }: { project: string | null | undefined }) {
  const said = useProjectSaid(project);
  const icon = useContext(ProjectIcons).get(said ?? '');
  if (!said) return null;
  return (
    <span className="ca-tag">
      {icon && <ProjectIcon src={icon} size={12} />}
      {said}
    </span>
  );
}

/** The project's cached org avatar when the window lists another or all projects, even if the
 *  place's own name matches the project's (so a chip's avatar replaces its generic folder/book
 *  icon consistently across main checkouts and worktrees). */
export function useProjectAvatar(project: string | null | undefined): string | null {
  const shown = useContext(ProjectShown);
  const icons = useContext(ProjectIcons);
  if (!project || shown == null || project === shown) return null;
  return icons.get(project) ?? null;
}

/** A place's project before its name, quieter than the name and followed by a slash, as a
 *  worktree's repo is said before it. `beside` are the names said after it. `withIcon` leaves out
 *  the org avatar where a chip already puts it in its leading icon slot. */
export function ProjectPrefix({ project, beside, withIcon = true }: { project: string | null | undefined; beside?: (string | null | undefined)[]; withIcon?: boolean }) {
  const said = useProjectSaid(project, beside);
  const icons = useContext(ProjectIcons);
  const icon = withIcon ? icons.get(said ?? '') : undefined;
  if (!said) return null;
  return (
    <span className="ca-projprefix" title={`In ${said}`}>
      {icon && <ProjectIcon src={icon} size={12} />}
      <span className="ca-projprefix-name">{said}</span>
    </span>
  );
}
