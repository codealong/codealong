// Checks a web link's right-click menu (menus.ts `linkMenu`): in a session's conversation, a link
// in a reply and the page a call made each offer Open link, then each of Chrome's profiles, then
// Copy link; picking a profile opens the link in it (`open_in_chrome`, by its folder), and one
// that doesn't open says why beside the link. Profiles that share a name are told apart by the
// account signed in to them, and a Chrome with one profile is just Open in Chrome. The rest of a
// reply keeps its own menu, and the page a session made, in its header, has the link's. The
// conversation and the profiles are stand-ins, on the first session:
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/link-menu-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var PAGE = 'https://claude.ai/artifact/ExampleArtifactId00002';
  var DOCS = 'https://example.com/docs/roles';
  var profiles = [
    { id: 'Personal', name: 'Personal', dir: 'Default', email: 'me@example.com' },
    { id: 'Work', name: 'Work', dir: 'Profile 3', email: 'me@work.example' },
  ];
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'chrome_profiles') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(JSON.parse(JSON.stringify(profiles)));
    }
    // Chrome won't open the Work profile.
    if (cmd === 'open_in_chrome' && (args || {}).dir === 'Profile 3') {
      window.__calls.push([cmd, args, options]);
      return Promise.reject('Couldn’t open it in Chrome’s Work profile: Chrome didn’t answer.');
    }
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 9 * 60e3;
      var item = function (kind, text, n, extra) { return Object.assign({ at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
      return Promise.resolve({ before: null, after: 20, items: [
        item('you', 'Lay out the roles and publish the plan', 0),
        item('tool', 'Artifact · `roles.html`', 1, { id: 't1', input: '{}' }),
        item('result', '', 2, { id: 't1', output: 'Published at ' + PAGE, link: { url: PAGE, title: 'Configurable roles', note: 'Updated · version 4' } }),
        item('agent', 'The plan is at ' + PAGE + ', and the roles follow ' + DOCS + '.', 3),
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !s.visitor && !s.archived) {
            picked = { key: s.key, lane: l.id };
            s.artifacts = [{ url: PAGE, title: 'Configurable roles', note: 'Updated · version 4', at: Date.now() - 60e3 }];
          }
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var link = function (url, within) { return all('.ca-sessinsp ' + (within || '') + ' a.ca-md-a').filter(function (a) { return a.getAttribute('href') === url; })[0]; };
  var WANT = ['Open link', 'Open in Chrome (Personal)', 'Open in Chrome (Work)', 'Copy link'];

  (async function () {
    try {
      await until(function () { return row(); }, 20000, 'no session to show');
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      await until(function () { return link(DOCS); }, 10000, 'the stand-in conversation didn’t show');

      var labels = await menuOf(link(DOCS));
      check('a link in a reply offers each of Chrome’s profiles', labels.join(' | ') === WANT.join(' | '), labels.join(' | '));
      check('reading Chrome’s profiles as it’s asked for', calls('chrome_profiles').length === 1, String(calls('chrome_profiles').length));
      menuItem('Open in Chrome (Personal)')();
      await until(function () { return calls('open_in_chrome').length === 1; }, 3000, 'picking a profile opened nothing');
      var sent = calls('open_in_chrome')[0][1];
      check('picking one opens the link in that profile, by its folder', sent.url === DOCS && sent.dir === 'Default', JSON.stringify(sent));

      await menuOf(link(DOCS));
      menuItem('Open link')();
      await until(function () { return calls('open_url').length === 1; }, 3000, 'Open link opened nothing');
      check('Open link opens it as a click does', calls('open_url')[0][1].url === DOCS, JSON.stringify(calls('open_url')[0][1]));

      await menuOf(link(DOCS));
      menuItem('Open in Chrome (Work)')();
      await until(function () { return q('.ca-sessinsp .ca-thread-agent .ca-md-file-why'); }, 3000, 'a profile that didn’t open said nothing');
      check('one that doesn’t open says why beside the link', /Work profile/.test(q('.ca-sessinsp .ca-thread-agent .ca-md-file-why').textContent), q('.ca-sessinsp .ca-thread-agent .ca-md-file-why').textContent);

      labels = await menuOf(link(PAGE, '.ca-thread-made'));
      check('the page a call made has the same menu', labels.join(' | ') === WANT.join(' | '), labels.join(' | '));

      var said = link(DOCS).closest('.ca-thread-said');
      labels = await menuOf(said.querySelector('.ca-thread-agent') || said);
      check('the rest of the reply keeps its own menu', labels.indexOf('Copy message') >= 0 && labels.indexOf('Open link') < 0, labels.join(' | '));

      var chip = q('.ca-sessinsp-links .ca-artifactchip');
      check('the header shows the page the session made', !!chip);
      if (chip) {
        labels = await menuOf(chip);
        check('and a right-click on it has the link’s menu', labels.join(' | ') === WANT.join(' | '), labels.join(' | '));
      }

      // Two profiles of one name are told apart by who's signed in; one profile alone needs no name.
      profiles = [
        { id: 'Default', name: 'Me', dir: 'Default', email: 'me@example.com' },
        { id: 'Profile 3', name: 'Me', dir: 'Profile 3', email: null },
      ];
      labels = await menuOf(link(DOCS));
      check('profiles sharing a name are told apart', labels.join(' | ') === 'Open link | Open in Chrome (Me · me@example.com) | Open in Chrome (Me · Profile 3) | Copy link', labels.join(' | '));
      profiles = [profiles[0]];
      labels = await menuOf(link(DOCS));
      check('with one profile it’s Open in Chrome', labels.join(' | ') === 'Open link | Open in Chrome | Copy link', labels.join(' | '));
      profiles = [];
      labels = await menuOf(link(DOCS));
      check('with none, the link still opens and copies', labels.join(' | ') === 'Open link | Copy link', labels.join(' | '));
    } catch (e) {
      check(String(e.message || e), false);
    }
    done();
  })();
})();
