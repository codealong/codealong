// Opens the first Claude Code session in Sessions on a stand-in conversation, for a screenshot of
// how one reads: your prompts at the right, the agent's replies with code blocks in their
// languages' colors (a diff's too), commands you ran, a prompt with a table wider than the
// conversation, and the prompt behind what's in view pinned over the top. thread-look-check.js
// checks it.
//   PREVIEW_INIT=scripts/preview/thread-look-init.js PREVIEW_NAME=thread-look scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) return;
  var at = picked.lastActivity || Date.now();
  var n = 0;
  var item = function (kind, text, more) {
    n += 100;
    var it = { at: at - (2000 - n) * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 };
    for (var k in more || {}) it[k] = more[k];
    return it;
  };
  var fence = '```';
  var items = [
    item('you', 'Why does the parser drop nested lists in the northwind notes?'),
    item('agent', [
      'It reads an item only as far as its first line, so whatever is indented under it starts a new block:',
      '',
      fence + 'ts',
      '// src/parse.ts',
      'export function itemAt(lines: string[], i: number): Item {',
      '  const text = lines[i].replace(/^\\s*[-*]\\s+/, \'\');',
      '  return { text, children: [] }; // nothing under it is kept',
      '}',
      fence,
      '',
      'I’ll gather the indented lines into the item’s children.',
    ].join('\n')),
    item('you', 'Go ahead, and run the tests once it’s in.'),
    item('agent', [
      'Done. The fix is in `itemAt`; to run the suite yourself:',
      '',
      fence + 'bash',
      'cd ~/projects/northwind-app && npm test -- parse',
      fence,
      '',
      'The engine side reads the same lists, so it changed too:',
      '',
      fence + 'rust',
      'fn item_at(lines: &[&str], i: usize) -> Item {',
      '    let depth = indent_of(lines[i]);',
      '    Item { text: lines[i].trim().to_string(), children: under(lines, i, depth) }',
      '}',
      fence,
    ].join('\n')),
    item('you', '', { input: 'git status --short', shell: true, id: 'ran' }),
    item('result', '', { output: ' M src/parse.ts\n M src-tauri/src/parse.rs', id: 'ran' }),
    item('you', '', { input: 'ls', shell: true, id: 'ls' }),
    item('result', '', { output: 'src', id: 'ls' }),
    item('agent', ['The change to the UI parser:', '', fence + 'diff', '@@ -12,2 +12,3 @@', '-  return { text, children: [] };', '+  const children = under(lines, i);', '+  return { text, children };', fence].join('\n')),
    item('you', 'Which notes had nested lists?\n\n| northwind_quarterly_planning_notes_section | kestrel_release_checklist_with_owners |\n| --- | --- |\n| 3 | 1 |'),
    item('you', 'Looks right. Write it up for the atlas team:\n\n- what was wrong\n- what changed, in both places\n- how to check it'),
    item('agent', 'Here’s the write-up. Nested items under a list item were read as blocks of their own; they’re now kept as the item’s children, in the UI’s parser and the engine’s alike, and `npm test -- parse` covers both.'),
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'session_page' && args && args.key === picked.key) return Promise.resolve({ before: null, after: n + 100, items: items });
    return real.apply(this, arguments);
  };
  var step = 0;
  var t = setInterval(function () {
    var row = document.querySelector('[id$="::' + picked.key + '"]');
    if (step === 0 && row) {
      step = 1;
      // The board's own theme wins over the scheme Chrome is given; a dark run asks for dark.
      if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        board.theme = 'dark';
        window.__emit('board', board);
      }
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return;
    }
    var thread = document.querySelector('.ca-sessinsp .ca-thread');
    var colored = thread && thread.querySelectorAll('.ca-md pre .ca-syn-keyword').length >= 2;
    if (step === 1 && colored) {
      // The first prompt gone off the top, so it's pinned there.
      var replies = thread.querySelectorAll('.ca-thread-said.is-agent');
      if (replies.length > 0) thread.scrollTop = replies[0].offsetTop - thread.offsetTop + 40;
      // On virtual time no frame is drawn, so no scroll event comes to pin the prompt.
      thread.dispatchEvent(new Event('scroll'));
      window.__threadLook = picked.key;
      clearInterval(t);
    }
  }, 100);
})();
