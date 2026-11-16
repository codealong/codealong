// Puts two starts whose sessions aren't here yet under the first session the list shows (one
// stuck on what its terminal shows, with its choices to answer it with, one starting) and nothing
// else under it, the session needing you for the stuck one as the engine has it (starting.rs
// `overlay`), selects that session and opens what's folded under its title: for a screenshot of
// the lines and the session's row (starting-check.js checks the lines).
//   PREVIEW_INIT=scripts/preview/starting-init.js PREVIEW_NAME=starting scripts/preview.sh sessions
(function () {
  var MIN = 60000;
  var screen = '  Hooks need review\n  3 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.\n\n› 1. Review hooks\n  2. Trust all and continue\n  3. Continue without trusting (hooks won’t run)\n\n  enter confirm · esc skip';
  var run = async function () {
    var first = await until(function () { return q('.ca-lane.is-agent[id^="row-"]'); }, 20000);
    var b = await window.__TAURI_INTERNALS__.invoke('get_board');
    var shown = null;
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!shown && (first.id === 'row-' + s.key || first.id.slice(-(s.key.length + 2)) === '::' + s.key)) shown = { lane: l, s: s }; }); });
    if (!shown) return;
    var now = Date.now();
    var key = shown.s.key;
    var choices = [{ number: 1, words: 'Review hooks' }, { number: 2, words: 'Trust all and continue' }, { number: 3, words: 'Continue without trusting (hooks won’t run)' }];
    var stuck = { at: now - 5 * MIN, account: 'codex', harness: 'Codex', lane: shown.lane.id, place: shown.lane.callsign, helper: true, opens: 'Draft the release notes for northwind from the merged PRs', terminal: 'Rex', stuck: true, screen: screen, pid: 4242, choices: choices };
    var starting = Object.assign({}, stuck, { at: now - MIN, opens: 'Check the migration on a copy of the staging database', stuck: false, screen: null, pid: null, choices: [] });
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.origin && s.origin.key === key) s.origin = null; if (s.helper && s.helper.parent === key) s.helper = null; }); });
    if (b.sides) delete b.sides[key];
    b.starting = {};
    b.starting[key] = [starting, stuck];
    Object.assign(shown.s, { status: 'needs', need: 'start', excerpt: 'Its helper waits · Codex: Hooks need review', statusSince: now - 2 * MIN, asks: false, stalledSince: null });
    // The board's own theme wins over the scheme Chrome is given; a dark run asks for dark.
    if (window.matchMedia('(prefers-color-scheme: dark)').matches) b.theme = 'dark';
    window.__emit('board', b);
    first.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    // Its own header, not the one shown before it.
    await until(function () { return q('.ca-sessinsp-kin.is-starting'); }, 5000);
    var more = await until(function () { return q('.ca-sessinsp-kids-more'); }, 5000);
    more.click();
  };
  run().catch(function () {});
})();
