// Checks the pages on the web a session made (artifacts Claude Code published) in its panel's
// header (SessionHeader.tsx, ArtifactBits.tsx): each is a chip among its links, the latest first,
// which a click opens in the browser; the first three are shown, as its PRs are, and "+N" after
// them, which names the rest on hover, shows them all as chips beside the others. The shown
// project's sessions keep their own where they made more than three, and the others are given
// five made up, so the session the window selects first has some past the three.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/artifacts-check.js
(function () {
  var SHOWN = 3;
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
          if ((s.artifacts || []).length > SHOWN) real++;
          else {
            s.artifacts = [
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-e', title: 'Release notes for ' + s.shortId, note: 'Updated · version 3', at: now - 5 * 60000 },
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-d', title: 'A dashboard of what ' + s.shortId + ' measured, with a title long enough to be cut short', note: 'Published · version 1', at: now - 3 * 3600000 },
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-c', title: 'Design notes', note: 'Created', at: now - 2 * 86400000 },
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-b', title: 'Kestrel launch plan', note: 'Published · version 2', at: now - 3 * 86400000 },
              { url: 'https://claude.ai/artifact/check-' + s.shortId + '-a', title: 'Atlas onboarding', note: 'Opened', at: now - 4 * 86400000 },
            ];
          }
          byKey[s.key] = s;
        });
      });
      return b;
    });
  };

  var chips = function () { return all('.ca-sessinsp-links .ca-artifactchip'); };
  var titles = function () { return chips().map(function (c) { return c.querySelector('.ca-artifactchip-title').textContent; }); };
  // The "+N" after the page chips, as against the one after the PRs.
  var more = function () {
    var last = chips()[chips().length - 1];
    var next = last && last.nextElementSibling;
    return next && next.classList.contains('ca-sessinsp-more') ? next : null;
  };
  var run = async function () {
    await until(function () { return chips().length > 0; }, 20000, 'an artifact chip in the header');
    await wait(300);
    // The session whose header it is: the one whose latest page the first chip names.
    var heading = (q('.ca-sessinsp-title') || {}).textContent;
    var first = titles()[0];
    var s = Object.keys(byKey).map(function (k) { return byKey[k]; }).filter(function (x) { return x.title === heading && x.artifacts && x.artifacts[0] && x.artifacts[0].title === first; })[0];
    note(real + ' sessions of this Mac had more than ' + SHOWN + ' of their own');
    check('the first chip names the session’s latest page', !!s, first + ' in ' + heading);
    if (!s) return done();
    var made = s.artifacts;
    // Its colors are its own: a sheen and a berry, where a neutral chip would read as a label.
    var look = getComputedStyle(chips()[0]);
    var berry = getComputedStyle(document.documentElement).getPropertyValue('--artifact').trim();
    var probe = document.createElement('span');
    probe.style.color = berry;
    document.body.appendChild(probe);
    var berryRgb = getComputedStyle(probe).color;
    probe.remove();
    check('a chip is a sheen in the artifact berry, not a neutral chip', /linear-gradient/.test(look.backgroundImage) && look.color === berryRgb && getComputedStyle(chips()[0].querySelector('.ca-icon')).color === berryRgb, look.backgroundImage + ' ' + look.color);
    check('the first three are chips, the latest first', titles().join(' | ') === made.slice(0, SHOWN).map(function (a) { return a.title; }).join(' | '), titles().join(' | '));
    check('each one’s tooltip says what was done with it and how to open it', chips().every(function (c, i) { return c.title.indexOf(made[i].title + (made[i].note ? ' · ' + made[i].note : '')) === 0 && /Click to open it in the browser\.$/.test(c.title); }), chips().map(function (c) { return c.title; }).join(' | '));
    check('the rest are counted after them, as a PR’s are', !!more() && more().textContent.trim() === '+' + (made.length - SHOWN), more() && more().textContent);
    check('and named on hover', !!more() && more().title === made.slice(SHOWN).map(function (a) { return a.title; }).join('\n'), more() && more().title);
    check('nothing lists them under the links', !q('.ca-sessinsp-head .ca-artifacts') && all('.ca-sessinsp-head .ca-sessinsp-artifacts-more').length === 0);
    chips()[1].click();
    await wait(200);
    check('a click on a chip opens its page', calls('open_url').length === 1 && last('open_url').url === made[1].url, JSON.stringify(sent('open_url')));
    more().click();
    await wait(300);
    check('“+N” shows them all as chips, the latest first', titles().join(' | ') === made.map(function (a) { return a.title; }).join(' | '), titles().join(' | '));
    check('and goes', !more());
    var head = q('.ca-sessinsp-head').getBoundingClientRect();
    check('every chip stays inside the header, however many', chips().every(function (c) { var r = c.getBoundingClientRect(); return r.right <= head.right + 1 && r.left >= head.left - 1; }));
    chips()[made.length - 1].click();
    await wait(200);
    check('one shown by it opens too', calls('open_url').length === 2 && last('open_url').url === made[made.length - 1].url, JSON.stringify(sent('open_url')));
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
