// Checks how a conversation sets your prompts apart from code (thread-look-init.js's stand-in):
// your prompts sit at the right, as wide as what they say, and so does the one pinned over the
// top; a code block is outlined, not in your prompts' tint, and in its language's colors (a
// diff's added and removed lines too), one with no language plain; a short command you ran leaves
// its Copy and Run room of their own; a table in a prompt wider than the conversation scrolls in
// its box rather than pushing the conversation sideways.
//   PREVIEW_WAIT=60000 PREVIEW_INIT=scripts/preview/thread-look-init.js scripts/preview.sh sessions '' scripts/preview/thread-look-check.js
(function () {
  var thread = function () { return q('.ca-sessinsp .ca-thread'); };
  var box = function (el) { return el.getBoundingClientRect(); };
  /** The thread's content box: where its rows lie, between its paddings. */
  var room = function () {
    var el = thread();
    var r = box(el);
    var cs = getComputedStyle(el);
    return { left: r.left + parseFloat(cs.paddingLeft), right: r.left + el.clientWidth - parseFloat(cs.paddingRight) };
  };
  /** The code block whose words have `text`. */
  var block = function (text) { return all('.ca-sessinsp .ca-codeblock').filter(function (b) { return b.querySelector('pre').textContent.indexOf(text) >= 0; })[0]; };
  var tones = function (b) { return all('[class^="ca-syn-"]', b).map(function (s) { return s.className; }); };

  until(function () { return window.__threadLook && thread(); }, 40000, 'the stand-in conversation, colored')
    .then(function () { return until(function () { return q('.ca-sessinsp .ca-thread-pinned'); }, 5000, 'the prompt pinned over the top'); })
    .then(function () {
      var r = room();
      var prompts = all('.ca-sessinsp .ca-thread-said.is-you > .ca-thread-you');
      var short = prompts.filter(function (p) { return /^Go ahead/.test(p.textContent); })[0];
      check('there are prompts', prompts.length >= 3, String(prompts.length));
      check('each sits at the right', prompts.every(function (p) { return Math.abs(box(p).right - r.right) < 1.5; }), prompts.map(function (p) { return Math.round(r.right - box(p).right); }).join(','));
      check('a short one is as wide as what it says, not the whole row', short && box(short).width < (r.right - r.left) * 0.75, short ? Math.round(box(short).width) + ' of ' + Math.round(r.right - r.left) : 'none');
      check('a long one leaves room at its left', prompts.every(function (p) { return box(p).left - r.left >= 23; }), prompts.map(function (p) { return Math.round(box(p).left - r.left); }).join(','));

      var pinned = q('.ca-sessinsp .ca-thread-pinned');
      check('the pinned prompt hangs at the right', Math.abs(box(pinned).right - r.right) < 1.5 && box(pinned).left - r.left >= 23, Math.round(box(pinned).left - r.left) + '…' + Math.round(r.right - box(pinned).right));

      var ts = block('export function itemAt');
      var pre = ts && ts.querySelector('pre');
      var cs = pre && getComputedStyle(pre);
      var tint = getComputedStyle(prompts[0]).backgroundColor;
      check('a code block is outlined', cs && cs.borderTopWidth === '1px' && cs.borderTopStyle === 'solid', cs && cs.borderTopWidth + ' ' + cs.borderTopStyle);
      check('not in your prompts’ tint', cs && cs.backgroundColor !== tint, cs && cs.backgroundColor + ' / ' + tint);
      check('TypeScript is in its colors', ts && tones(ts).indexOf('ca-syn-keyword') >= 0 && tones(ts).indexOf('ca-syn-comment') >= 0, ts ? tones(ts).join(' ') : 'no block');
      check('its words are its code’s, as written', pre && pre.textContent.indexOf("return { text, children: [] }; // nothing under it is kept") >= 0, pre && pre.textContent);
      var rust = block('fn item_at');
      check('Rust is in its colors', rust && tones(rust).indexOf('ca-syn-keyword') >= 0, rust ? tones(rust).join(' ') : 'no block');
      var diff = block('@@ -12,2');
      var removed = diff && all('.ca-syn-keyword', diff).map(function (s) { return s.textContent; });
      var added = diff && all('.ca-syn-tag', diff).map(function (s) { return s.textContent; });
      check('a diff’s removed lines are red, its added ones green', removed && /^-/.test(removed[0]) && added && added.length === 2 && added.every(function (a) { return /^\+/.test(a); }), diff ? JSON.stringify([removed, added]) : 'no block');

      var ran = all('.ca-sessinsp .ca-thread-ran').filter(function (el) { return /! ls/.test(el.textContent); })[0];
      var cmd = ran && ran.querySelector('.ca-codeblock');
      var words = cmd && document.createRange();
      if (words) words.selectNodeContents(cmd.querySelector('pre'));
      var tools = cmd && cmd.querySelector('.ca-codeblock-tools');
      check('a short command’s Copy and Run leave its words be', words && tools && words.getBoundingClientRect().right <= box(tools).left, words && tools ? Math.round(words.getBoundingClientRect().right) + ' > ' + Math.round(box(tools).left) : 'no command');

      var table = q('.ca-sessinsp .ca-thread-you .ca-md-table');
      check('a wide table in a prompt scrolls in its box', table && table.scrollWidth > table.clientWidth, table ? table.scrollWidth + ' in ' + table.clientWidth : 'no table');
      check('and the conversation doesn’t scroll sideways', thread().scrollWidth <= thread().clientWidth, thread().scrollWidth + ' in ' + thread().clientWidth);
      done();
    })
    .catch(stopped);
})();
