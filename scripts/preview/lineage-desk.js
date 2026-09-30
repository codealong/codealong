// preview.sh lineage: the window of lines between desktop cards (DesktopLines.tsx) over a
// stand-in desktop, with stand-ins for the cards drawn over it, as their own windows are in the
// app. The pointer is on "Plan the release": the session it was forked from is across from it, and
// under it, in the column new cards stack in, a fork of it and a session its agent started.
window.__CODEALONG_LINEAGE__ = true;
(function () {
  var card = function (x, y, title) { return { x: x, y: y, w: 280, h: 56, title: title }; };
  var hovered = card(1120, 40, 'Plan the release');
  var cards = [hovered, card(1120, 120, 'Plan the release ⑂'), card(1120, 200, 'Write the changelog'), card(620, 300, 'Release checklist')];
  var box = function (c) { return { x: c.x, y: c.y, w: c.w, h: c.h }; };
  window.__LINEAGE_FEED__ = {
    theme: 'system',
    lines: [
      { from: box(cards[3]), to: box(hovered), side: 'left' },
      { from: box(hovered), to: box(cards[1]), side: 'left' },
      { from: box(hovered), to: box(cards[2]), side: 'left' },
    ],
  };
  document.addEventListener('DOMContentLoaded', function () {
    var style = document.createElement('style');
    style.textContent =
      'html.is-lineage{background:linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))!important}' +
      '.stand-in{position:fixed;z-index:1;box-sizing:border-box;padding:10px 12px;border-radius:10px;background:var(--surface-raised);color:var(--ink);font:500 13px var(--font-sans);box-shadow:var(--shadow-lift)}' +
      '.stand-in.is-hovered{outline:2px solid var(--focus)}';
    document.head.appendChild(style);
    cards.forEach(function (c) {
      var el = document.createElement('div');
      el.className = 'stand-in' + (c === hovered ? ' is-hovered' : '');
      el.style.left = c.x + 'px';
      el.style.top = c.y + 'px';
      el.style.width = c.w + 'px';
      el.style.height = c.h + 'px';
      el.textContent = c.title;
      document.body.appendChild(el);
    });
  });
})();
