// Checks what's shown of Codealong's automatic reviews (autoreview.rs, `[reviews]`) and your draft
// review, in a PR panel's tabs (PrInspector.tsx `PrDetails`). The latest open PR is made one that
// waits on your review, whose agent reviewed an earlier commit into a draft review of yours, with
// new commits since: its panel opens on its Reviews tab (agentReview.ts `openingTab`), which counts
// the draft's comments, beside Description and Changes, which counts its files. The tab says
// where the latest round stands (review.ts `autoReviewWords`), its Draft link the PR's files
// (open_url) and, when the PR lists a session, the session. Then your draft as GitHub has it
// (pr_draft, stubbed; AgentReview.tsx `DraftReview`): its text as Markdown, the line saying an
// agent wrote it in italics, then each comment with its file, folder and line, the lines around
// it from the PR's diff (pr_detail, stubbed) with its own marked, its place opening that line on
// GitHub at the draft's commit; an outdated one says so and has no lines. A comment is changed
// (edit_draft_comment) and shows what it says now; Remove asks for a second click, then takes it
// out (drop_draft_comment), and the counts drop; the text is changed (edit_draft). The box fixed
// at the panel's foot (PrBox.tsx) has the line over it saying where the review stands and what
// the agent suggests (asking for changes, its button marked so), which shows the Reviews tab from
// another; Approve there with your words sends review_pr, the toast says the draft went with its
// comment, and once GitHub has no draft but your approval of the agent's review, the tab says you
// submitted it and shows what the agent wrote (pr_agent_reviews, stubbed), the review of an
// earlier commit a click away, saying whose and when. The file count goes to Changes. The next
// PR's review didn't finish: it opens on Description, and its Reviews tab says why in red. The
// third is still being reviewed, so it's held out of what waits on you (prStatus.ts
// `heldForAgent`): its row says Agent reviewing in place of Review requested, its Reviews tab has a
// dot, and says it waits on you once that's done. A fourth is made a PR of yours an agent
// reviewed (`[reviews.own]`) with handing back off: it opens on Reviews, says why its review hasn't
// gone back, and Send hands it to the session that opened it (send_agent_review). Then Settings ›
// Automation (no longer Accounts) has the switch, off by default, with the PR nudge, the usage
// limit and the network error; off, the reviewer and where reviews go still show, since Review
// with… starts the same review; turned on it saves `[reviews]` as config.toml has it, and the
// reviewer is a box drawn as a session's (AgentPromptBox.tsx): what it's told, Codealong's own
// instructions until you change them, and the agent chip at its foot, whose menu is the quick
// panel's (Auto, then each account that can review out of the way with its models). Picking an
// account saves it; what you write is saved as the box loses the keyboard, and going back to
// Codealong's saves none. Each review goes to a draft first. Reviews of the PRs your sessions open
// have a switch of their own, off by default, which saves `[reviews.own]` handing reviews back,
// with a reviewer box of its own.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/auto-review-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var LINK = 'https://github.com/o/r/pull/1/files';
  var drafted = null;
  var draftedNumber = null;
  var PATCH = '@@ -1,4 +1,5 @@\n fn a() {\n-    old();\n+    new();\n+    more();\n }';
  var WRITTEN = [
    {
      head: '309811aaaabbbb',
      stage: 'drafted',
      at: Date.now() - 5 * 60000,
      session: null,
      agent: 'Claude Code, claude-opus-5-5',
      link: LINK,
      summary: 'I’d hold this until the **first** finding is fixed.',
      comments: [
        { path: 'src/a.rs', line: 2, body: 'This panics on empty input: check `len` first.' },
        { path: 'docs/guide.md', line: 40, body: 'Say what changed here too.' },
      ],
    },
    { head: '1111111aaaa', stage: 'posted', at: Date.now() - 3 * 3600000, session: null, agent: 'Codex, gpt-5.5', link: 'https://github.com/o/r/pull/1#issuecomment-9', summary: 'An earlier review.', comments: [] },
  ];
  var DEFAULT = 'Look for bugs, security and data risks, missing tests, and anything that doesn’t do what the PR says it does. Read the code around the changes too, and raise where they don’t fit it in a way that will cost whoever works on it next.';
  // Your draft on GitHub, as pr_draft has it, until it's submitted.
  var DRAFT = {
    id: 'PRR_1',
    body: 'I’d hold this until the **first** finding is fixed.\n\n<sub>An agent’s review (Claude Code, claude-opus-5-5) of 309811a, drafted by Codealong.</sub>',
    commit: '309811aaaabbbbcccc',
    url: null,
    count: 2,
    comments: [
      { id: 'PRRC_1', path: 'src/a.rs', line: 2, outdated: false, body: 'This panics on empty input: check `len` first.' },
      { id: 'PRRC_2', path: 'docs/guide.md', line: 40, outdated: true, body: 'Say what changed here too.' },
    ],
  };
  var submitted = null;
  var prUrl = null;
  var yours = null;
  var yoursKey = null;
  var posted = null;
  var failed = null;
  var held = null;
  var session = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  // Settings are kept as saved, as the engine keeps config.toml: the stub answers nothing to a save.
  var saved = null;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'pr_agent_reviews') {
      return Promise.resolve(args.key === drafted ? WRITTEN : []);
    }
    if (cmd === 'pr_draft') {
      var ours = args.number === draftedNumber;
      return Promise.resolve({ draft: ours && !submitted ? JSON.parse(JSON.stringify(DRAFT)) : null, submitted: ours ? submitted : null });
    }
    if (cmd === 'edit_draft_comment') {
      DRAFT.comments.forEach(function (c) { if (c.id === args.id) c.body = args.body; });
      return Promise.resolve(null);
    }
    if (cmd === 'drop_draft_comment') {
      DRAFT.comments = DRAFT.comments.filter(function (c) { return c.id !== args.id; });
      DRAFT.count = DRAFT.comments.length;
      return Promise.resolve(null);
    }
    if (cmd === 'edit_draft') {
      DRAFT.body = args.text;
      return Promise.resolve(null);
    }
    if (cmd === 'review_pr' && args.number === draftedNumber) {
      submitted = { state: 'APPROVED', at: Date.now(), url: 'https://github.com/o/r/pull/1#pullrequestreview-7' };
      return Promise.resolve({ draft: DRAFT.count });
    }
    if (cmd === 'pr_detail' && args.number === draftedNumber) {
      return Promise.resolve({ bodyHtml: '', mergeable: null, commits: 1, reviews: [], requested: [], comments: [], files: [{ path: 'src/a.rs', status: 'modified', additions: 2, deletions: 1, patch: PATCH }], moreFiles: false });
    }
    if (cmd === 'save_config') {
      saved = args.config;
      return Promise.resolve(null);
    }
    if (cmd === 'get_config') {
      // As a config.toml without [reviews] has it, whatever this Mac's says; and an engine from
      // before the instructions could be edited says none.
      return p.then(function (view) {
        var config = Object.assign({}, view.config);
        delete config.reviews;
        DEFAULT = view.reviewPrompt || DEFAULT;
        return Object.assign({}, view, { reviewPrompt: DEFAULT, config: saved || config });
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; }).sort(function (a, b) { return b.sessions.length - a.sessions.length || b.updatedAt - a.updatedAt; });
      var now = Date.now();
      if (open[0]) {
        var pr = open[0];
        session = pr.sessions[0] ? pr.sessions[0].key : null;
        drafted = pr.key;
        draftedNumber = pr.number;
        WRITTEN[0].session = session;
        Object.assign(pr, { state: 'open', mine: false, author: 'ana', reviewRequested: true, snoozedUntil: null, headOid: '2222222bbbbcccc' });
        pr.autoReview = { stage: 'drafted', head: '309811aaaabbbb', parts: session ? [{ who: 'auto', session: session, state: 'done' }] : [], at: now - 5 * 60000, comment: LINK, rounds: 2, behind: true, suggests: 'request-changes' };
        posted = '#' + pr.number;
        prUrl = pr.url;
      }
      if (open[1]) {
        Object.assign(open[1], { state: 'open', mine: false, reviewRequested: true, snoozedUntil: null });
        open[1].autoReview = { stage: 'failed', head: '4444444dddd', at: now - 60000, note: 'claude can’t start a session in the background.', rounds: 1 };
        failed = '#' + open[1].number;
      }
      if (open[2]) {
        Object.assign(open[2], { state: 'open', mine: false, reviewRequested: true, snoozedUntil: null, merge: 'blocked' });
        open[2].autoReview = { stage: 'reviewing', head: '5555555eeee', parts: [{ who: 'auto', state: 'reviewing' }], at: now - 30000, rounds: 1, holds: true };
        held = '#' + open[2].number;
      }
      // A PR of yours, which an agent on another model reviewed with handing back turned off.
      var maker = open[3] && open[3].sessions[0];
      if (maker) {
        Object.assign(open[3], { state: 'open', mine: true, reviewRequested: false, snoozedUntil: null, primary: maker.key });
        open[3].autoReview = { stage: 'kept', head: '6666666ffff', parts: [{ who: 'auto', state: 'done' }], at: now - 120000, rounds: 1, own: true, sendable: maker.key, note: 'Handing reviews back is off: send it from here.' };
        yours = '#' + open[3].number;
        yoursKey = open[3].key;
      }
      board.pinnedPrs = [];
      return board;
    });
  };
  var number = function (row) { return (q('.ca-prrow-num', row) || {}).textContent; };
  var row = function (n) { return all('.ca-prrow').filter(function (r) { return number(r) === n; })[0]; };
  var auto = function () { return q('.ca-prinsp .ca-prinsp-auto'); };
  var tab = function (label) { return all('.ca-prinsp .ca-tab').filter(function (t) { return t.firstChild && t.firstChild.textContent === label; })[0]; };
  var shownTab = function () { var t = q('.ca-prinsp .ca-tab.is-on'); return t && t.firstChild.textContent; };
  var tabCount = function (label) { var c = q('.ca-tab-count', tab(label)); return c ? c.textContent : null; };
  var hidden = function (el) { return !el || !!el.closest('[hidden]'); };
  var type = function (el, text) {
    var proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var linkbtn = function (label) { return all('.ca-prinsp-auto .ca-linkbtn').filter(function (b) { return b.textContent === label; })[0]; };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var box = function (words) { return all('.ca-settings-check').filter(function (l) { return words.test(l.textContent); })[0]; };

  (async function () {
    try {
      await until(function () { return posted && row(posted); }, 20000, 'no open PR to give a review');
      press(row(posted));
      await until(function () { return !!auto() && !!tab('Reviews'); }, 5000, 'the PR panel shows no automatic review');
      var labels = all('.ca-prinsp .ca-tab').map(function (t) { return t.firstChild.textContent; });
      check('the PR’s parts are tabs: Description, Changes and Reviews', labels.join(' | ') === 'Description | Changes | Reviews', labels.join(' | '));
      check('it opens on Reviews, as the agent’s review waits there', shownTab() === 'Reviews' && !hidden(auto()) && hidden(q('.ca-prinsp [aria-label="Description"]')), shownTab());
      var fileCount = q('.ca-prinsp .ca-prinsp-files') || q('.ca-prinsp .ca-prinsp-size');
      check('Changes counts its files', !!tabCount('Changes') && fileCount.textContent.indexOf(tabCount('Changes') + ' file') >= 0, tabCount('Changes') + ' / ' + fileCount.textContent);
      var says = q('.ca-prinsp-auto-says').textContent;
      check('it says the round and its commit', says === 'Agent drafted a review of 309811a', says);
      await until(function () { return !!q('.ca-agentrev-draft'); }, 5000, 'your draft on GitHub never showed');
      check('once the draft shows, its own On GitHub is the way there, not Draft', !linkbtn('Draft') && !!button('On GitHub', q('.ca-agentrev-drafthead')), all('.ca-prinsp-auto .ca-linkbtn').map(function (b) { return b.textContent; }).join(', '));
      press(button('On GitHub', q('.ca-agentrev-drafthead')));
      check('which opens the PR’s files, where your draft is', calls('open_url').some(function (c) { return c[1].url === prUrl + '/files'; }), JSON.stringify(calls('open_url')));
      var next = (q('.ca-prinsp-auto-note') || {}).textContent;
      check('that new commits get another, the draft saying that only you see it', next === 'New commits since: it reviews them again once they settle.', next);
      check('Reviews counts the draft’s comments', tabCount('Reviews') === '2', tabCount('Reviews'));
      if (session) {
        check('Session is there for the PR’s session', !!linkbtn('Session'), all('.ca-prinsp-auto .ca-linkbtn').map(function (b) { return b.textContent; }).join(', '));
      } else {
        note('no PR lists a session, so Session isn’t checked');
      }
      var draft = q('.ca-agentrev-draft');
      check('your draft says its comments, and that only you see it', q('.ca-agentrev-drafttitle', draft).textContent === 'Your draft review · 2 comments' && /^Only you see it\. Approve, Comment or Request changes in the box below submits it/.test(q('.ca-agentrev-draftnote', draft).textContent), q('.ca-agentrev-drafthead', draft).textContent);
      var text = q('.ca-agentrev-drafttext .ca-agentrev-text', draft);
      check('its text, as Markdown, the line saying an agent wrote it in italics', !!q('strong', text) && /drafted by Codealong\.$/.test((q('em', text) || {}).textContent || '') && !/<sub>/.test(text.textContent), text.innerHTML.slice(0, 160));
      await until(function () { return all('.ca-agentrev-comment').length === 2; }, 5000, 'the draft’s comments never showed');
      var first = all('.ca-agentrev-comment')[0];
      check('each comment says its file, folder and line', q('.ca-agentrev-file', first).textContent === 'a.rs' && q('.ca-agentrev-dir', first).textContent === 'src' && q('.ca-agentrev-line', first).textContent === 'line 2', first.textContent.slice(0, 60));
      await until(function () { return !!q('.ca-agentrev-code', first); }, 5000, 'no lines around the first comment');
      var at = q('.ca-agentrev-code .is-at', first);
      check('with the lines around it, its own marked', all('.ca-agentrev-code > span', first).length === 5 && !!at && /\+    new\(\);/.test(at.textContent), q('.ca-agentrev-code', first).textContent);
      // Each a line of its own, an added one tinted as a diff's are (PrFiles.css draws the box).
      var plain = getComputedStyle(q('.ca-agentrev-code', first)).backgroundColor;
      check('each a line of its own, an added one tinted', getComputedStyle(at).display === 'block' && getComputedStyle(at).backgroundColor !== plain && getComputedStyle(at).backgroundColor !== 'rgba(0, 0, 0, 0)', getComputedStyle(at).display + ' ' + getComputedStyle(at).backgroundColor);
      press(q('.ca-agentrev-where', first));
      check('its place opens that line on GitHub at the draft’s commit', calls('open_url').some(function (c) { return /\/blob\/309811aaaabbbbcccc\/src\/a\.rs#L2$/.test(c[1].url); }), JSON.stringify(calls('open_url').slice(-1)));
      var second = all('.ca-agentrev-comment')[1];
      check('an outdated comment says so, and has no lines', !q('.ca-agentrev-code', second) && (q('.ca-tag', second) || {}).textContent === 'Outdated', second.textContent.slice(0, 80));

      // A comment changed, on GitHub.
      press(button('Edit', first));
      await until(function () { return !!q('textarea', first); }, 3000, 'Edit opened no box');
      check('Edit opens a box with what it says', q('textarea', first).value === 'This panics on empty input: check `len` first.', q('textarea', first).value);
      type(q('textarea', first), 'This panics on an empty list.');
      press(button('Save', first));
      await until(function () { return calls('edit_draft_comment').length === 1 && !q('textarea', all('.ca-agentrev-comment')[0]); }, 3000, 'Save sent nothing');
      check('Save changes it on GitHub', JSON.stringify(calls('edit_draft_comment')[0][1]) === JSON.stringify({ id: 'PRRC_1', body: 'This panics on an empty list.' }), JSON.stringify(calls('edit_draft_comment')[0][1]));
      await until(function () { return /This panics on an empty list\./.test(all('.ca-agentrev-comment')[0].textContent); }, 3000, 'the comment doesn’t say what it says now');
      // One taken out, after a second click.
      second = all('.ca-agentrev-comment')[1];
      press(button('Remove', second));
      await until(function () { return !!button('Click again to remove', second); }, 2000, 'Remove asked for no second click');
      check('Remove asks for a second click first', !!button('Click again to remove', second) && calls('drop_draft_comment').length === 0, second.textContent.slice(-40));
      press(button('Click again to remove', second));
      await until(function () { return calls('drop_draft_comment').length === 1 && all('.ca-agentrev-comment').length === 1; }, 3000, 'the comment wasn’t taken out');
      check('the second takes it out, on GitHub', calls('drop_draft_comment')[0][1].id === 'PRRC_2', JSON.stringify(calls('drop_draft_comment')[0][1]));
      await until(function () { return tabCount('Reviews') === '1'; }, 3000, 'Reviews still counts 2');
      check('and the counts drop', q('.ca-agentrev-drafttitle').textContent === 'Your draft review · 1 comment', q('.ca-agentrev-drafttitle').textContent);
      // Its text changed.
      press(button('Edit text'));
      await until(function () { return !!q('.ca-agentrev-draft .ca-agentrev-edit textarea'); }, 3000, 'Edit text opened no box');
      var textBox = q('.ca-agentrev-draft .ca-agentrev-edit textarea');
      check('Edit text opens a box with its text as it goes, signed', /^I’d hold this/.test(textBox.value) && /<sub>An agent’s review/.test(textBox.value), textBox.value.slice(0, 80));
      type(textBox, 'Hold it for the first one.');
      press(button('Save', q('.ca-agentrev-draft .ca-agentrev-edit')));
      await until(function () { return calls('edit_draft').length === 1; }, 3000, 'Save sent no text');
      check('Save gives the draft its text on GitHub', calls('edit_draft')[0][1].text === 'Hold it for the first one.' && calls('edit_draft')[0][1].number === draftedNumber, JSON.stringify(calls('edit_draft')[0][1]));
      // Submitted from the box at the panel's foot.
      var verdict = q('.ca-prpanel > .ca-prreview-box');
      check('the panel has the box at its foot, out of what scrolls', !!verdict && !verdict.closest('.ca-prpanel-body') && !!q('.ca-prpanel-body .ca-prinsp-title, .ca-prpanel-body .ca-insp-call'), '');
      var line = q('.ca-prbox-line', verdict);
      check('over it, where the review stands, with your draft and what the agent suggests', !!line && /^Agent drafted a review of 309811a: your draft review, 1 comment/.test(q('.ca-prbox-line-says', line).textContent) && (q('.ca-prbox-suggests', line) || {}).textContent === 'Suggests changes', line ? line.textContent : 'no line');
      check('the button the agent suggests says so', /^The agent suggests this\./.test(button('Request changes', verdict).title) && !/suggests/.test(button('Approve', verdict).title), button('Request changes', verdict).title);
      check('and a verdict submits the draft', /submitting your draft review and its 1 comment/.test(button('Approve', verdict).title), button('Approve', verdict).title);
      press(tab('Description'));
      await until(function () { return shownTab() === 'Description'; }, 2000, 'Description never showed');
      press(line);
      await until(function () { return shownTab() === 'Reviews'; }, 2000, 'the line didn’t show the Reviews tab');
      check('a click on the line shows the Reviews tab', shownTab() === 'Reviews', shownTab());
      type(q('textarea', verdict), 'LGTM once that’s fixed.');
      press(button('Approve', verdict));
      await until(function () { return calls('review_pr').length === 1; }, 3000, 'Approve sent nothing');
      var sent = calls('review_pr')[0][1];
      check('Approve sends your words, which go before the draft’s text', sent.verdict === 'approve' && sent.body === 'LGTM once that’s fixed.' && sent.number === draftedNumber, JSON.stringify(sent));
      await until(function () { return /submitting your draft review and its 1 comment/.test(document.body.textContent); }, 3000, 'no toast says the draft went');
      await until(function () { return !!q('.ca-agentrev-submitted'); }, 5000, 'the tab doesn’t say it was submitted');
      check('then the tab says you submitted it', /^You submitted the agent’s review, approving it/.test(q('.ca-agentrev-submitted').textContent) && q('.ca-prinsp-auto-note').textContent.indexOf('You submitted it, approving it.') === 0, q('.ca-agentrev-submitted').textContent + ' / ' + q('.ca-prinsp-auto-note').textContent);
      await until(function () { return !!q('.ca-agentrev-body'); }, 5000, 'what the agent wrote never showed');
      check('and what the agent wrote', /The agent’s review \(Claude Code, claude-opus-5-5\) · 2 comments on lines/.test(q('.ca-agentrev-byline').textContent) && /I’d hold this until the first finding is fixed\./.test(q('.ca-agentrev-body > .ca-agentrev-text').textContent), q('.ca-agentrev-byline').textContent);
      var rounds = all('.ca-agentrev-rounds button').map(function (b) { return b.textContent; });
      check('the review of each commit is a click away, the latest first', rounds.join(' | ') === '309811a, the latest | 1111111', rounds.join(' | '));
      press(all('.ca-agentrev-rounds button')[1]);
      await until(function () { return !!q('.ca-agentrev-byline'); }, 3000, 'the earlier review never showed');
      await until(function () { return /^Of 1111111/.test(q('.ca-agentrev-byline').textContent); }, 3000, 'the earlier review never showed');
      check('an earlier review says whose and when', /^Of 1111111, 3h ago, by Codex, gpt-5\.5/.test(q('.ca-agentrev-byline').textContent) && all('.ca-agentrev-comment').length === 0, q('.ca-agentrev-byline').textContent);
      press(q('.ca-prinsp .ca-prinsp-files'));
      await until(function () { return shownTab() === 'Changes'; }, 3000, 'the file count didn’t go to Changes');
      check('the file count goes to Changes, and its files show', !hidden(q('.ca-prinsp .ca-prfiles-sec')) && hidden(auto()), shownTab());
      press(tab('Description'));
      await until(function () { return shownTab() === 'Description'; }, 2000, 'Description never showed');
      check('Description shows its description', shownTab() === 'Description' && !hidden(q('.ca-prinsp [aria-label="Description"]')), shownTab());
      if (failed && row(failed)) {
        press(row(failed));
        await until(function () { return !!tab('Reviews') && q('.ca-prinsp .ca-insp-call').textContent === failed; }, 5000, 'the second PR never showed');
        check('a PR whose review didn’t finish opens on its description', shownTab() === 'Description', shownTab());
        press(tab('Reviews'));
        await until(function () { return auto() && /didn’t finish/.test(auto().textContent); }, 5000, 'the second PR shows no failed review');
        var red = getComputedStyle(q('.ca-prinsp-auto-says')).color;
        var alert = getComputedStyle(document.documentElement).getPropertyValue('--alert').trim();
        check('a round that didn’t finish says why', q('.ca-prinsp-auto-note').textContent === 'claude can’t start a session in the background.', q('.ca-prinsp-auto-note').textContent);
        check('in the alert colour', !!red && red !== getComputedStyle(q('.ca-prinsp-auto-head .ca-meta')).color, red + ' vs ' + alert);
        check('with no Comment or Session to go to', all('.ca-prinsp-auto .ca-linkbtn').length === 0, '');
      }
      if (held && row(held)) {
        var pills = all('.ca-prpill-text', row(held)).map(function (p) { return p.textContent.trim(); });
        check('a PR the agent is still on says so on its row', pills.some(function (t) { return t === 'Agent reviewing'; }), pills.join(', '));
        press(row(held));
        await until(function () { return !!tab('Reviews') && q('.ca-prinsp .ca-insp-call').textContent === held; }, 5000, 'the third PR never showed');
        check('its Reviews tab has a dot while the agent reviews it', !!q('.ca-tab-busy', tab('Reviews')), '');
        press(tab('Reviews'));
        await until(function () { return auto() && /Agent reviewing 5555555/.test(auto().textContent); }, 5000, 'the held PR shows no review under way');
        var why = q('.ca-prinsp-auto-note').textContent;
        check('and its panel that it waits on your review once that’s done', why === 'It waits on your review once the agent’s is done.', why);
      } else {
        note('no third open PR to hold for the agent');
      }
      if (yours && row(yours)) {
        press(row(yours));
        await until(function () { return !!tab('Reviews') && q('.ca-prinsp .ca-insp-call').textContent === yours; }, 5000, 'your PR never showed');
        check('a PR of yours an agent reviewed opens on Review', shownTab() === 'Reviews', shownTab());
        await until(function () { return auto() && /Agent reviewed 6666666/.test(auto().textContent); }, 5000, 'your PR shows no agent review');
        var sendTo = all('.ca-prinsp-auto .ca-linkbtn').filter(function (b) { return /^Send to “/.test(b.textContent); })[0];
        check('it says why its review hasn’t gone back, and offers to send it to the session that opened it', !!sendTo && q('.ca-prinsp-auto-note').textContent === 'Handing reviews back is off: send it from here.', all('.ca-prinsp-auto .ca-linkbtn').map(function (b) { return b.textContent; }).join(', '));
        press(sendTo);
        await until(function () { return calls('send_agent_review').length === 1; }, 3000, 'Send sent nothing');
        check('Send hands it to that session', calls('send_agent_review')[0][1].key === yoursKey, JSON.stringify(calls('send_agent_review')[0][1]));
      } else {
        note('no fourth open PR with a session to make yours');
      }

      // Review with… starts the same review by hand (review_pr_now): on the reviewer Settings
      // names, or an account picked, each where reviews run, out of sight.
      var reviewWith = function () { return all('.ca-prpanel button').filter(function (b) { return b.textContent.trim() === 'Review with…'; })[0]; };
      if (reviewWith()) {
        var withLabels = [];
        for (var tries = 0; tries < 5 && !withLabels.some(function (l) { return / in (the background|Codealong|a terminal behind your windows)/.test(l); }); tries++) {
          var menusBefore = menus();
          press(reviewWith());
          await until(function () { return menus() > menusBefore; }, 3000, 'Review with… opens no menu');
          withLabels = menuLabels();
          if (withLabels.length < 2) await wait(300);
        }
        check('Review with… lists where each review runs, out of sight, and no terminal you’d see', withLabels.length > 0 && withLabels.every(function (l) { return / in (the background|Codealong|a terminal behind your windows)(, as Settings says)?$/.test(l) || /^Auto \(another harness/.test(l) || l === 'The Codex app’s own review'; }), withLabels.join(' | '));
        var account = withLabels.filter(function (l) { return !/^Auto|as Settings says|Codex app/.test(l); })[0];
        if (account) {
          menuItem(account)();
          await until(function () { return calls('review_pr_now').length === 1; }, 3000, 'picking an account started no review');
          var asked = calls('review_pr_now')[0][1];
          check('picking an account starts Codealong’s review of the PR on it', /#\d+$/.test(asked.key) && asked.account === account.split(' ')[0], JSON.stringify(asked));
        } else {
          note('no account to pick in Review with…: ' + withLabels.join(' | '));
        }
      } else {
        note('no PR panel with Review with… open');
      }

      q('.ca-side-config').click();
      await until(function () { return !!nav('Automation'); }, 3000, 'no Automation in Settings');
      var on = /Review pull requests that wait on your review/;
      nav('Accounts').click();
      await until(function () { return !!q('.ca-settings-pane .ca-acct, .ca-settings-pane .ca-accounts, .ca-settings-pane fieldset, .ca-settings-pane section'); }, 5000, 'Settings › Accounts shows nothing');
      check('Settings › Accounts no longer has what Codealong does by itself', !box(on) && !box(/Carry on after a network error/) && !box(/Nudge the session that opened/), '');
      nav('Automation').click();
      await until(function () { return !!box(on); }, 5000, 'no reviews setting in Settings › Automation');
      check('Settings › Automation has it, with the nudge, the usage limit and the network error', !!box(/Nudge the session that opened/) && !!box(/Carry on after a network error/) && /When a session hits its usage limit/.test(q('.ca-settings-pane').textContent), '');
      var input = box(on).querySelector('input');
      check('reviews are off by default', !input.checked, String(input.checked));
      var field = function (words) { return all('.ca-field').filter(function (f) { return words.test((q('span', f) || {}).textContent || ''); })[0]; };
      check('off, the reviewer and where its reviews go still show, for Review with…', !!field(/^The reviewer$/) && !!field(/^Where each review goes$/) && !!field(/^Submit the agent’s review for you$/) && /Off, an agent reviews one when you ask, with Review with…/.test(box(on).textContent), '');
      input.click();
      await until(function () { return calls('save_config').length > 0; }, 3000, 'turning it on saved nothing');
      var cfg = calls('save_config')[0][1].config;
      check('turned on, it saves [reviews] as config.toml has it', cfg && cfg.reviews && cfg.reviews.auto === true && cfg.reviews.account === '' && cfg.reviews.model === '' && cfg.reviews.prompt === '' && cfg.reviews.post === 'draft', JSON.stringify(cfg && cfg.reviews));
      await until(function () { return !!field(/^The reviewer$/); }, 3000, 'no reviewer once it’s on');
      var where = all('option', field(/^Where each review goes$/)).map(function (o) { return o.textContent; });
      check('each review goes to a draft first, else a comment or nowhere', where.join(' | ') === 'A draft review for you to finish | A comment on the pull request | Nowhere: it stays in its session', where.join(' | '));
      // What of the agent's verdict is submitted for you is a select too, never by default.
      var submitField = function () { return field(/^Submit the agent’s review for you$/); };
      var submits = all('option', submitField()).map(function (o) { return o.textContent; });
      check('the agent’s verdict is submitted for you never by default, else as an approval, with changes requested, or with a comment for them, or those without approving', submits.join(' | ') === 'Never: you submit each one | Approve when it finds nothing to fix | Approve, or request changes | Approve, or comment instead of requesting changes | Request changes, never approve | Comment instead of requesting changes, never approve' && q('select', submitField()).value === 'never', submits.join(' | ') + ' / ' + q('select', submitField()).value);
      var submitSaves = calls('save_config').length;
      var submitSelect = q('select', submitField());
      submitSelect.value = 'approve-or-comment';
      submitSelect.dispatchEvent(new Event('change', { bubbles: true }));
      await until(function () { return calls('save_config').length > submitSaves; }, 3000, 'picking what’s submitted saved nothing');
      var submitCfg = calls('save_config')[calls('save_config').length - 1][1].config.reviews;
      check('picked, it saves [reviews] submit, and says what it does', submitCfg.submit === 'approve-or-comment' && !('auto_approve' in submitCfg) && /goes as a comment from you instead, which reaches the author without blocking the merge/.test(q('small', submitField()).textContent), JSON.stringify(submitCfg) + ' / ' + q('small', submitField()).textContent);
      // The reviewer is a box drawn as a session's: what it's told, and the agent chip at its foot.
      var agentBox = function () { return q('.ca-agentbox', field(/^The reviewer$/)); };
      var told = function () { return q('textarea', agentBox()); };
      var chip = function () { return q('.ca-modelmenu-chip', agentBox()); };
      check('the reviewer is a box with its instructions and the agent chip, as a session’s is', !!agentBox() && !!q('.ca-composer-field', agentBox()) && !!told() && !!chip(), '');
      check('it shows what the reviewer is told: Codealong’s instructions', told().value === DEFAULT, told().value);
      // Accounts come from start_options, which this Mac's snapshot answers.
      await until(function () { return !chip().disabled; }, 5000, 'the agent chip stays off: no account can review out of the way');
      check('the chip says Auto, and whom it would start', /^Auto/.test(chip().textContent), chip().textContent);
      chip().click();
      await until(function () { return !!q('.ca-picker', agentBox()); }, 3000, 'the chip opens no menu');
      var opts = all('.ca-picker-opt', agentBox());
      var heads = all('.ca-picker-head .ca-quick-acct-name', agentBox()).map(function (h) { return h.textContent; });
      check('its menu is the quick panel’s: Auto, then each account with its models', /Auto/.test(opts[0].textContent) && heads.length > 0 && opts.length > heads.length, heads.join(', '));
      note('accounts in the menu: ' + heads.join(', '));
      var picked = opts[1];
      var saves = calls('save_config').length;
      picked.click();
      await until(function () { return calls('save_config').length > saves; }, 3000, 'picking an account saved nothing');
      var chosen = calls('save_config')[saves][1].config.reviews;
      check('picking an account saves it as the reviewer, on the line’s model', chosen.account === heads[0] && typeof chosen.model === 'string', JSON.stringify(chosen));
      // What's typed goes in as React takes typing, and is kept as the box loses the keyboard.
      var mine = 'Start two subagents, one on Opus and one on Sonnet, and combine what they find.';
      check('with nothing to go back to yet', !all('.ca-linkbtn', agentBox()).some(function (b) { return /Codealong’s instructions/.test(b.textContent); }), '');
      saves = calls('save_config').length;
      told().focus();
      type(told(), mine);
      told().blur();
      await until(function () { return calls('save_config').length > saves; }, 3000, 'writing instructions saved nothing');
      check('instructions of your own are saved', calls('save_config')[saves][1].config.reviews.prompt === mine, JSON.stringify(calls('save_config')[saves][1].config.reviews));
      var back = function () { return all('.ca-linkbtn', agentBox()).filter(function (b) { return /Codealong’s instructions/.test(b.textContent); })[0]; };
      await until(function () { return !!back(); }, 3000, 'no way back to Codealong’s instructions');
      saves = calls('save_config').length;
      back().click();
      await until(function () { return calls('save_config').length > saves; }, 3000, 'going back saved nothing');
      check('and going back saves none, which is Codealong’s', calls('save_config')[saves][1].config.reviews.prompt === '', JSON.stringify(calls('save_config')[saves][1].config.reviews));
      await until(function () { return told() && told().value === DEFAULT; }, 3000, 'the box doesn’t show Codealong’s instructions again');

      // The pull requests your sessions open, reviewed on another model and handed back.
      var ownSwitch = function () { return box(/Review the pull requests your sessions open/); };
      check('your own pull requests have a switch of their own, off by default', !!ownSwitch() && !ownSwitch().querySelector('input').checked, '');
      check('with nothing more while it’s off', !field(/^Reviewer of your pull requests$/), '');
      saves = calls('save_config').length;
      ownSwitch().querySelector('input').click();
      await until(function () { return calls('save_config').length > saves; }, 3000, 'turning it on saved nothing');
      var ownCfg = calls('save_config')[saves][1].config.reviews.own;
      check('turned on, it saves [reviews.own], handing each review back', ownCfg && ownCfg.auto === true && ownCfg.send === true && ownCfg.account === '' && ownCfg.prompt === '', JSON.stringify(ownCfg));
      await until(function () { return !!field(/^Reviewer of your pull requests$/); }, 3000, 'no reviewer for your pull requests once it’s on');
      var theirs = q('.ca-agentbox', field(/^Reviewer of your pull requests$/));
      check('which is a box of its own, as the reviewer above is, whose Auto is another harness than each PR’s session’s', !!theirs && theirs !== agentBox() && /^Auto\s*another harness/.test(q('.ca-modelmenu-chip', theirs).textContent) && q('textarea', theirs).value === DEFAULT, theirs ? q('.ca-modelmenu-chip', theirs).textContent : 'none');
      var handBack = box(/Hand each review to the session that opened/);
      check('and the switch that hands reviews back, on', !!handBack && handBack.querySelector('input').checked, '');
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
