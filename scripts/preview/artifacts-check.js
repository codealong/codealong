// Checks the pages on the web a session made (artifacts Claude Code published) in its panel's
// header (SessionHeader.tsx, ArtifactBits.tsx): the latest is a chip among its links, which a
// click opens in the browser; "+N more" beside it, which lists the rest by title on hover, shows
// them in a list under the links, the latest first, each opening on a click, and "fewer" puts
// them away again. The shown project's sessions keep their own where they made two or more, and
// the others are given three made up, so the session the window selects first has some.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/artifacts-check.js
(function () {
  var byKey = {};
  var real = 0;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      var now = Date.now();
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if ((s.artifacts || []).length >= 2) real++;
          else {
            s.artifacts = [
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-c', title: 'Release notes for ' + s.shortId, note: 'Updated · version 3', at: now - 5 * 60000 },
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-b', title: 'A dashboard of what ' + s.shortId + ' measured, with a title long enough to be cut short', note: 'Published · version 1', at: now - 3 * 3600000 },
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-a', title: 'Design notes', note: 'Created', at: now - 2 * 86400000 },
            ];
          }
          byKey[s.key] = s;
        });
      });
      return b;
    });
  };

  var chip = function () { return q('.ca-sessinsp-links .ca-artifactchip'); };
  var more = function () { return q('.ca-sessinsp-links .ca-sessinsp-artifacts-more'); };
  var rows = function () { return all('.ca-sessinsp-head .ca-artifacts .ca-artifact'); };
  var run = async function () {
    await until(function () { return !!chip(); }, 20000, 'an artifact chip in the header');
    await wait(300);
    // The session whose header it is: the one whose latest page the chip names.
    var title = chip().querySelector('.ca-artifactchip-title').textContent;
    var heading = (q('.ca-sessinsp-title') || {}).textContent;
    var s = Object.keys(byKey).map(function (k) { return byKey[k]; }).filter(function (x) { return x.title === heading && x.artifacts && x.artifacts[0] && x.artifacts[0].title === title; })[0];
    note(real + ' sessions of this Mac had two or more of their own');
    check('the chip names the session’s latest page', !!s, title + ' in ' + heading);
    if (!s) return done();
    var made = s.artifacts;
    check('its tooltip says what was done with it and how to open it', chip().title.indexOf(made[0].title) === 0 && /Click to open it in the browser\.$/.test(chip().title), chip().title);
    check('the rest are counted beside it', !!more() && more().textContent.trim() === '+' + (made.length - 1) + ' more' && more().getAttribute('aria-expanded') === 'false', more() && more().textContent);
    check('and named on hover', !!more() && more().title === made.slice(1).map(function (a) { return a.title; }).join('\n'), more() && more().title);
    check('none is listed before it’s asked for', rows().length === 0, String(rows().length));
    chip().click();
    await wait(200);
    check('a click on the chip opens the page', calls('open_url').length === 1 && last('open_url').url === made[0].url, JSON.stringify(sent('open_url')));
    var line = chip().getBoundingClientRect();
    more().click();
    await wait(300);
    var listed = rows();
    check('“+N more” lists the rest under the links, the latest first', listed.length === made.length - 1 && listed.every(function (r, i) { return r.querySelector('.ca-artifact-title').textContent === made[i + 1].title; }), listed.map(function (r) { return r.textContent; }).join(' | '));
    check('each with what was last done with it', listed.every(function (r, i) { return !made[i + 1].note || (r.querySelector('.ca-artifact-note') || {}).textContent === made[i + 1].note; }));
    var links = q('.ca-sessinsp-links').getBoundingClientRect();
    var head = q('.ca-sessinsp-head').getBoundingClientRect();
    check('the list is under the links, inside the header', listed.length > 0 && listed[0].getBoundingClientRect().top >= links.bottom && listed.every(function (r) { return r.getBoundingClientRect().right <= head.right + 1; }));
    check('the toggle stays where it was, saying “fewer”', !!more() && more().textContent.trim() === 'fewer' && more().getAttribute('aria-expanded') === 'true' && Math.abs(more().getBoundingClientRect().top - line.top) < 6);
    if (listed[0]) listed[0].click();
    await wait(200);
    check('a click on one opens it', calls('open_url').length === 2 && last('open_url').url === made[1].url, JSON.stringify(sent('open_url')));
    more().click();
    await wait(300);
    check('“fewer” puts them away', rows().length === 0 && !!chip() && more().textContent.trim() === '+' + (made.length - 1) + ' more');
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
