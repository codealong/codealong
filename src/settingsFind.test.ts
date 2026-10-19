import { describe, expect, it } from 'vitest';
import { findWords, foundIn, says, spansIn, type Piece } from './settingsFind';

/** A setting, what isn't one (`chrome`), or what a section says now (`alert`), named by its text. */
const setting = (text: string, ...kids: Piece<string>[]): Piece<string> => ({ at: text, setting: true, alert: false, text, kids });
const chrome = (text: string, ...kids: Piece<string>[]): Piece<string> => ({ at: text, setting: false, alert: false, text, kids });
const alert = (text: string): Piece<string> => ({ at: text, setting: false, alert: true, text, kids: [] });

describe('what the words find', () => {
  it('finds every word, in any order and any case', () => {
    expect(says('Close a command’s tab once it ends', findWords('TAB close'))).toBe(true);
    expect(says('Close a command’s tab once it ends', findWords('close window'))).toBe(false);
  });

  it('takes a typed apostrophe for a curly one', () => {
    expect(says('Close a command’s tab', findWords("command's"))).toBe(true);
  });

  it('finds a short word only whole among others, as the quick panel does', () => {
    expect(says('Open at login', findWords('at'))).toBe(true);
    expect(says('Where reviews start', findWords('at review'))).toBe(false);
  });

  it('finds pull requests by PR', () => {
    expect(says('Review pull requests that wait on your review', findWords('pr review'))).toBe(true);
    expect(says('Nudge the session that opened a pull request', findWords('prs nudge'))).toBe(false);
  });
});

describe('where the words are', () => {
  it('marks each word every time it appears', () => {
    expect(spansIn('Sounds: the sound for each', findWords('sound'))).toEqual([
      [0, 5],
      [12, 17],
    ]);
  });

  it('marks what PR stands for', () => {
    expect(spansIn('Your pull request', findWords('pr'))).toEqual([[5, 17]]);
  });
});

describe('what of a section shows', () => {
  const pane = chrome(
    '',
    chrome('Automation', chrome('What Codealong does by itself')),
    chrome(
      '',
      setting('Review pull requests that wait on your review'),
      setting('Where reviews run', chrome('In your terminal'), chrome('In Codealong')),
      setting('Routing groups', chrome('Add routing group'), chrome('', setting('reviewers Round robin'), setting('fast Most room'))),
    ),
  );

  it('shows the settings that match and hides the rest, and what’s around them that isn’t a setting', () => {
    const found = foundIn(pane, findWords('review'), 'Automation');
    expect(found.count).toBe(3);
    expect(found.hidden).toContain('Automation');
    expect(found.hidden).not.toContain('Where reviews run');
  });

  it('finds a setting by the choices it offers', () => {
    const found = foundIn(pane, findWords('terminal'), 'Automation');
    expect(found.count).toBe(1);
    expect(found.hidden).toEqual(expect.arrayContaining(['Review pull requests that wait on your review', 'Routing groups']));
    expect(found.hidden).not.toContain('In Codealong');
  });

  it('shows a setting found by one inside it with only that one, and its own words', () => {
    const found = foundIn(pane, findWords('fast'), 'Automation');
    expect(found.count).toBe(1);
    expect(found.hidden).toContain('reviewers Round robin');
    expect(found.hidden).not.toContain('fast Most room');
    expect(found.hidden).not.toContain('Add routing group');
  });

  it('shows a setting whole when its own words match', () => {
    const found = foundIn(pane, findWords('groups'), 'Automation');
    expect(found.hidden).not.toContain('reviewers Round robin');
    expect(found.hidden).not.toContain('fast Most room');
  });

  it('shows every setting in a section found by its name', () => {
    const found = foundIn(pane, findWords('automation'), 'Automation');
    expect(found.count).toBe(3);
    expect(found.hidden).toEqual(['Automation']);
  });

  it('shows a setting whole when the words are spread over the ones inside it', () => {
    const found = foundIn(pane, findWords('reviewers fast'), 'Automation');
    expect(found.count).toBe(1);
    expect(found.hidden).not.toContain('reviewers Round robin');
    expect(found.hidden).not.toContain('fast Most room');
  });

  it('keeps a setting that showed for these words', () => {
    const found = foundIn(pane, findWords('terminal'), 'Automation', new Set(['Review pull requests that wait on your review']));
    expect(found.count).toBe(2);
    expect(found.shown).toContain('Review pull requests that wait on your review');
  });

  it('finds nothing where nothing matches', () => {
    expect(foundIn(pane, findWords('sounds'), 'Automation').count).toBe(0);
  });

  it('shows what a section says now wherever it is, an error beside a setting, a note in a list', () => {
    const general = chrome('', chrome('', setting('Open at login'), alert('Couldn’t reach the login items')), chrome('', alert('Turned on')));
    const found = foundIn(general, findWords('login'), 'General');
    expect(found.hidden).toEqual([]);
    expect(found.count).toBe(1);
  });

  it('shows a section with nothing that matches for what it says now', () => {
    const accounts = chrome('', chrome('Each subscription has its own config folder'), alert('Couldn’t read the accounts'));
    const found = foundIn(accounts, findWords('nudge'), 'Accounts');
    expect(found.count).toBe(0);
    expect(found.alerting).toBe(true);
    expect(found.hidden).toEqual(['Each subscription has its own config folder']);
    expect(foundIn(pane, findWords('nudge'), 'Automation').alerting).toBe(false);
  });

  it('shows a setting that’s an alert, whatever the words', () => {
    const asking: Piece<string> = { ...setting('kestrel wants to link'), alert: true };
    const found = foundIn(chrome('', setting('Link devices'), asking), findWords('nudge'), 'Devices');
    expect(found.hidden).toEqual(['Link devices']);
    expect(found.count).toBe(1);
  });
});
