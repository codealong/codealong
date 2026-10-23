import { describe, expect, it } from 'vitest';
import { ALL_PROJECTS } from '../quick';
import { placeWords, projectSaid } from './ProjectSaid';

describe('projectSaid', () => {
  it('says a place’s project under All projects, and under another project', () => {
    expect(projectSaid('Northwind', ALL_PROJECTS)).toBe('Northwind');
    expect(projectSaid('Northwind', 'Atlas')).toBe('Northwind');
  });

  it('says nothing under its own project, in a window that lists none, or of a place in none', () => {
    expect(projectSaid('Northwind', 'Northwind')).toBeNull();
    expect(projectSaid('Northwind', null)).toBeNull();
    expect(projectSaid('', ALL_PROJECTS)).toBeNull();
    expect(projectSaid(null, ALL_PROJECTS)).toBeNull();
  });

  it('leaves it to a name beside it that already is the project’s, whatever its case', () => {
    expect(projectSaid('Kestrel', ALL_PROJECTS, ['kestrel', 'fix-login'])).toBeNull();
    expect(projectSaid('Kestrel', ALL_PROJECTS, ['kestrel-app', null])).toBe('Kestrel');
  });
});

describe('placeWords', () => {
  it('puts the project first, as project / worktree', () => {
    expect(placeWords('Northwind', 'fix-login', ALL_PROJECTS)).toBe('Northwind / fix-login');
    expect(placeWords('Northwind', 'fix-login', 'Northwind')).toBe('fix-login');
    expect(placeWords('Kestrel', 'kestrel', ALL_PROJECTS)).toBe('kestrel');
  });
});
