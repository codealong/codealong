// Checks a start whose session isn't here yet (`Board.starting`, starting.rs):
// - nested under its session in the Sessions list (`StartingListRow`) and under its session's title
//   (`StartingRow`), with no inline action buttons crowding the parent's header;
// - clicking the nested row or header line opens its panel (`StartingPanel`), which shows its
//   terminal screen, choices (`start_answer`), Show in its terminal (`open_target`), and Done
//   (`start_stop`, ⇧⌘E);
// - a stuck start's notice (Notices.tsx) still shows its screen as laid out.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/starting-check.js
// A window that shows a session as a card (`SessionCard`) has the line too, and clicking it opens
// `StartingPanel` inside the card with choices, Show in its terminal, Done and Back:
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/starting-check.js
(function () {
  var MIN = 60000;
  var screen = '  Hooks need review\n  3 hooks are new or changed.\n\n› 1. Review hooks\n  2. Trust all and continue\n\n  enter confirm · esc skip';
  var opens = 'Draft the release notes for northwind from the merged PRs';
  var choices = [{ number: 1, words: 'Review hooks' }, { number: 2, words: 'Trust all and continue' }];
  var row = function () { return q('.ca-sessinsp-kids .ca-sessinsp-kin'); };
  var rows = function () { return all('.ca-sessinsp-kids .ca-sessinsp-kin'); };
  var text = function (el, sel) { var x = el && el.querySelector(sel); return x ? x.textContent : null; };
  var panelChoices = function () { return all('.ca-startpanel-choices .ca-btn'); };
  var panelBtn = function (words) { return all('.ca-startpanel .ca-btn, .ca-startpanel .ca-hotkey').find(function (x) { return x.textContent && x.textContent.trim().indexOf(words) === 0; }); };

  // The peek: the session it shows, with a start under its title; clicking opens StartingPanel.
  var peek = async function () {
    var card = await until(function () { return q('[data-shows-session]'); }, 20000, 'the peek’s session');
    var key = card.getAttribute('data-shows-session');
    var b = await window.__TAURI_INTERNALS__.invoke('get_board');
    var home = null;
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) home = l; if (s.origin && s.origin.key === key) s.origin = null; if (s.helper && s.helper.parent === key) s.helper = null; }); });
    if (b.sides) delete b.sides[key];
    b.starting = {};
    b.starting[key] = [{ at: Date.now() - 5 * MIN, account: 'codex', harness: 'Codex', lane: home ? home.id : '/r', place: home ? home.callsign : 'r', helper: true, opens: opens, terminal: 'Rex', stuck: true, screen: screen, pid: 4242, choices: choices }];
    window.__emit('board', b);
    var r = await until(function () { return q('.ca-sessinsp-kids .ca-sessinsp-kin.is-starting'); }, 5000, 'the start under the peek’s title');
    check('the peek lists the start under its session too', text(r, '.ca-sessinsp-kin-title') === opens && text(r, '.ca-sessinsp-kin-state') === '\u201cHooks need review\u201d', r.textContent);
    check('with no inline action buttons in the parent header', !q('.ca-sessinsp-start-acts'));
    r.click();
    await until(function () { return q('.ca-startpanel'); }, 5000, 'StartingPanel in peek');
    check('clicking it opens its card with choices, Show in Rex and Done', panelChoices().map(function (x) { return x.textContent; }).join(' | ') === 'Review hooks | Trust all and continue' && !!panelBtn('Show in Rex') && !!panelBtn('Done'), all('.ca-startpanel .ca-btn').map(function (x) { return x.textContent; }).join(' | '));
    done();
  };

  var run = async function () {
    if (window.__CODEALONG_PEEK__) return peek();
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
    var stuck = { at: now - 5 * MIN, account: 'codex', harness: 'Codex', lane: shown.lane.id, place: shown.lane.callsign, helper: true, opens: opens, terminal: 'Rex', stuck: true, screen: screen, pid: 4242, choices: choices };
    var older = Object.assign({}, stuck, { at: now - 40 * MIN, helper: false, opens: null, stuck: false, screen: null, pid: null, choices: [] });
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.origin && s.origin.key === key) s.origin = null; if (s.helper && s.helper.parent === key) s.helper = null; }); });
    if (b.sides) delete b.sides[key];
    b.starting = {};
    b.starting[key] = [stuck, older];
    b.notices = (b.notices || []).concat([{ key: 'start:' + stuck.at, title: 'Codex hasn’t started in ' + shown.lane.callsign, text: 'Codex was started in Rex 5 minutes ago, in ' + shown.lane.callsign + ', and hasn’t made its session yet: its terminal shows this, and waits on it to be answered before it starts. Answer it in that tab; if you closed it, dismiss this.', screen: screen }]);
    window.__emit('board', b);
    first.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    await until(function () { return row() && row().classList.contains('is-starting'); }, 5000, 'the start under the title');

    var nested = all('.ca-lane.is-agent.is-nested.is-starting');
    check('the pending starts appear nested under their session in Sessions, stuck first', nested.length === 2 && nested[0].classList.contains('is-stuck') && text(nested[0], '.ca-agentline-title') === opens && text(nested[0], '.ca-startingline-state') === '\u201cHooks need review\u201d', nested.map(function (x) { return x.textContent; }).join(' | '));

    var r = row();
    check('the latest start hangs under the title, by how its prompt opens', text(r, '.ca-sessinsp-kin-title') === opens, text(r, '.ca-sessinsp-kin-title'));
    check('stuck, it says what its terminal waits on', text(r, '.ca-sessinsp-kin-state') === '\u201cHooks need review\u201d' && r.classList.contains('is-stuck'), text(r, '.ca-sessinsp-kin-state'));
    check('with a glyph that waits on you, and how long since it was started', !!r.querySelector('.ca-glyph') && /^[45]m$/.test(text(r, '.ca-sessinsp-kin-age')), text(r, '.ca-sessinsp-kin-age'));
    check('no inline action buttons clutter the parent header', !q('.ca-sessinsp-start-acts'));
    check('the line is a clickable button', r.tagName === 'BUTTON' && !r.disabled, r.tagName);
    var state = r.querySelector('.ca-sessinsp-kin-state').getBoundingClientRect();
    var title = r.querySelector('.ca-sessinsp-kin-title').getBoundingClientRect();
    check('its title, then its state, on one line', Math.abs(state.top - title.top) < 4 && state.left >= title.right - 1, Math.round(title.right) + ' / ' + Math.round(state.left));

    var more = q('.ca-sessinsp-kids-more');
    check('the older one waits behind “+1 more”', !!more && more.textContent === '+1 more' && /Started, starting: Codex on codex/.test(more.title), more && more.title);
    more.click();
    await until(function () { return rows().length === 2; }, 3000, 'both starts');
    var second = rows()[1];
    check('opened, the older one says it is starting, titled by its harness without a prompt, and is clickable too', text(second, '.ca-sessinsp-kin-state') === 'Starting' && text(second, '.ca-sessinsp-kin-title') === 'Codex on codex' && !second.classList.contains('is-stuck') && second.tagName === 'BUTTON' && !second.disabled, second.tagName + ' ' + second.textContent);

    // Clicking the nested row opens StartingPanel and highlights the nested row.
    nested[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    await until(function () { return q('.ca-startpanel'); }, 5000, 'StartingPanel opened from nested row');
    check('the nested row highlights as selected while its panel is open', nested[0].classList.contains('is-selected') && !first.classList.contains('is-selected'));
    var scr = q('.ca-startpanel-screen');
    check('the panel shows its terminal screen and choices', !!scr && scr.textContent === screen && panelChoices().map(function (x) { return x.textContent; }).join(' | ') === 'Review hooks | Trust all and continue', panelChoices().map(function (x) { return x.textContent; }).join(' | '));
    panelChoices()[1].click();
    await until(function () { return calls('start_answer').length > 0; }, 3000, 'the answer');
    var answered = calls('start_answer')[0][1];
    check('a choice answers what its screen asked, by its number and words', answered.at === stuck.at && answered.asked === 'Hooks need review' && answered.number === 2 && answered.said === 'Trust all and continue', JSON.stringify(answered));
    var ready = function () { return until(function () { return panelChoices().every(function (x) { return !x.disabled; }); }, 3000, 'the buttons back'); };
    await ready();
    panelBtn('Show in Rex').click();
    await until(function () { return calls('open_target').length > 0; }, 3000, 'the show');
    var shownAt = calls('open_target')[0][1];
    check('Show in Rex opens its tab, under its worktree', shownAt.laneId === shown.lane.id && shownAt.targetId === 'start|' + stuck.at + '|focus', JSON.stringify(shownAt));
    await ready();
    panelBtn('Done').click();
    await until(function () { return calls('start_stop').length > 0; }, 3000, 'the stop');
    check('Done stops it and returns to the parent session', calls('start_stop')[0][1].at === stuck.at, JSON.stringify(calls('start_stop')[0][1]));
    await until(function () { return !q('.ca-startpanel') && !!row(); }, 5000, 'back on the parent session');

    // Clicking the header line opens StartingPanel too.
    row().click();
    await until(function () { return q('.ca-startpanel'); }, 5000, 'StartingPanel opened from header line');
    check('clicking the header line opens the same panel', !!q('.ca-startpanel-screen'));

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
