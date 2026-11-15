// Checks a start whose session isn't here yet (`Board.starting`, starting.rs) in its session's
// header (SessionHeader.tsx `StartingRow`): under the title as what came of it, titled by how its
// prompt opens, the latest first; stuck, it says what its terminal waits on in place of
// "Starting", its tooltip has the whole screen, and it's no button. The stuck start's notice
// (Notices.tsx) shows that screen as it's laid out. The board is the snapshot's, with two starts
// put under the first session the list shows, and the notice the engine would make for the stuck one.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/starting-check.js
(function () {
  var MIN = 60000;
  var screen = '  Hooks need review\n  3 hooks are new or changed.\n\n› 1. Review hooks\n  2. Trust all and continue\n\n  enter confirm · esc skip';
  var opens = 'Draft the release notes for northwind from the merged PRs';
  var row = function () { return q('.ca-sessinsp-kids .ca-sessinsp-kin'); };
  var rows = function () { return all('.ca-sessinsp-kids .ca-sessinsp-kin'); };
  var text = function (el, sel) { var x = el && el.querySelector(sel); return x ? x.textContent : null; };

  var run = async function () {
    // The session the list shows first (App.tsx `domIdOf`: its row's id is `row-` and its row's
    // key, which ends in `::` and the session's key).
    var first = await until(function () { return q('.ca-lane.is-agent[id^="row-"]'); }, 20000, 'a session in the list');
    var b = await window.__TAURI_INTERNALS__.invoke('get_board');
    var shown = null;
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!shown && (first.id === 'row-' + s.key || first.id.slice(-(s.key.length + 2)) === '::' + s.key)) shown = { lane: l, s: s }; }); });
    if (!shown) {
      check('a listed session to start from', false, first.id);
      return done();
    }
    var now = Date.now();
    var key = shown.s.key;
    var stuck = { at: now - 5 * MIN, account: 'codex', harness: 'Codex', lane: shown.lane.id, place: shown.lane.callsign, helper: true, opens: opens, terminal: 'Rex', stuck: true, screen: screen };
    var older = Object.assign({}, stuck, { at: now - 40 * MIN, helper: false, opens: null, stuck: false, screen: null });
    // Under its session alone, whatever came of it already: what this checks is the starts.
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.origin && s.origin.key === key) s.origin = null; if (s.helper && s.helper.parent === key) s.helper = null; }); });
    if (b.sides) delete b.sides[key];
    b.starting = {};
    b.starting[key] = [stuck, older];
    b.notices = (b.notices || []).concat([{ key: 'start:' + stuck.at, title: 'Codex hasn’t started in ' + shown.lane.callsign, text: 'Codex was started in Rex 5 minutes ago, in ' + shown.lane.callsign + ', and hasn’t made its session yet: its terminal shows this, and waits on it to be answered before it starts. Answer it in that tab; if you closed it, dismiss this.', screen: screen }]);
    window.__emit('board', b);
    first.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    await until(function () { return row() && row().classList.contains('is-starting'); }, 5000, 'the start under the title');

    var r = row();
    check('the latest start hangs under the title, by how its prompt opens', text(r, '.ca-sessinsp-kin-title') === opens, text(r, '.ca-sessinsp-kin-title'));
    check('stuck, it says what its terminal waits on', text(r, '.ca-sessinsp-kin-state') === '“Hooks need review”' && r.classList.contains('is-stuck'), text(r, '.ca-sessinsp-kin-state'));
    // The page goes by its own clock, which on virtual time can be a little behind this one.
    check('with a glyph that waits on you, and how long since it was started', !!r.querySelector('.ca-glyph') && /^[45]m$/.test(text(r, '.ca-sessinsp-kin-age')), text(r, '.ca-sessinsp-kin-age'));
    check('its tooltip has the whole screen and where to answer it', r.title.indexOf(screen) >= 0 && /in Rex, in /.test(r.title) && /Answer it in that tab/.test(r.title), r.title);
    check('it is no button: there is nothing to show yet', r.tagName === 'DIV');
    var state = r.querySelector('.ca-sessinsp-kin-state').getBoundingClientRect();
    var title = r.querySelector('.ca-sessinsp-kin-title').getBoundingClientRect();
    check('its title, then its state, on one line', Math.abs(state.top - title.top) < 4 && state.left >= title.right - 1, Math.round(title.right) + ' / ' + Math.round(state.left));

    var more = q('.ca-sessinsp-kids-more');
    check('the older one waits behind “+1 more”', !!more && more.textContent === '+1 more' && /Started, starting: Codex on codex/.test(more.title), more && more.title);
    more.click();
    await until(function () { return rows().length === 2; }, 3000, 'both starts');
    var second = rows()[1];
    check('opened, the older one says it is starting, titled by its harness without a prompt', text(second, '.ca-sessinsp-kin-state') === 'Starting' && text(second, '.ca-sessinsp-kin-title') === 'Codex on codex' && !second.classList.contains('is-stuck'), second.textContent);

    q('.ca-notice-open').click();
    await until(function () { return q('.ca-notice-card'); }, 3000, 'the notices card');
    var item = all('.ca-notice-item').find(function (s) { return /hasn’t started in/.test(s.textContent); });
    var pre = item && item.querySelector('.ca-notice-screen');
    check('the stuck start’s notice shows its screen as it’s laid out', !!pre && pre.textContent === screen && getComputedStyle(pre).whiteSpace === 'pre-wrap', pre && pre.textContent);
    var card = q('.ca-notice-card').getBoundingClientRect();
    check('within the card', !!pre && pre.getBoundingClientRect().right <= card.right + 0.5, pre && Math.round(pre.getBoundingClientRect().right) + ' / ' + Math.round(card.right));
    done();
  };
  run().catch(function (e) { check('ran', false, String(e)); done(); });
})();
