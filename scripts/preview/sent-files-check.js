// Checks the files a call sent you (Claude Code's SendUserFile) in a conversation (SentFiles.tsx):
// each picture, film and recording shows in place, loaded from where it is (preview.sh links a
// transcript's sent files under ./files), no taller than a few lines, with its name under it no
// wider than it; any other file the call sent, or one that's gone, is a line of its own. Run it on
// a session whose page (PREVIEW_BEFORE, when it's gone on since) has a call that sent you a
// picture or a film that's still there.
//   PREVIEW_WAIT=30000 PREVIEW_TRANSCRIPT=<key> PREVIEW_BEFORE=<cursor> scripts/preview.sh sessions '' scripts/preview/sent-files-check.js
(function () {
  // Every project's sessions are listed, so PREVIEW_TRANSCRIPT's is wherever it works.
  localStorage.setItem('codealong.project', 'All projects');
  var media = function () { return all('.ca-thread-sentmedia > li'); };
  var loaded = function (li) {
    var img = li.querySelector('img');
    var av = li.querySelector('video, audio');
    if (img) return img.complete && img.naturalWidth > 0;
    if (av) return av.readyState >= 1;
    // One macOS asks about first waits for a click, and one that can't be shown says so.
    return !!li.querySelector('.is-note');
  };

  say('sent files check: opening the conversation');
  fetch('./transcript-key.json')
    .then(function (r) { return r.ok ? r.json() : null; })
    .catch(function () { return null; })
    .then(function (fixed) {
      if (!fixed) throw new Error('no PREVIEW_TRANSCRIPT: name a session that sent you a picture or a film');
      var row = function () { return document.querySelector('[id$="::' + fixed.key + '"]'); };
      return until(row, 20000, 'its row').then(function () { row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true })); });
    })
    .then(function () { return until(function () { return media().length && media(); }, 20000, 'a sent picture, film or recording in the conversation'); })
    .then(function (lis) {
      say('waiting for ' + lis.length + ' to load');
      return until(function () { return lis.every(loaded) && lis; }, 20000, 'every one loaded').catch(function (e) {
        // Say which didn't, by its name, rather than only that one didn't.
        var not = lis.filter(function (li) { return !loaded(li); }).map(function (li) { return li.querySelector('.ca-md-file-name').textContent; });
        throw new Error(e.message + ': ' + not.join(', '));
      });
    })
    .then(function (lis) {
      lis.forEach(function (li) {
        var name = li.querySelector('figcaption .ca-md-file-name');
        var label = name ? name.textContent : '(no name)';
        var shown = li.querySelector('img, video, audio, .is-note');
        var box = shown.getBoundingClientRect();
        check(label + ' has its name under it', !!name);
        check(label + ' is no taller than a few lines', box.height <= 241, Math.round(box.height) + 'px');
        check(label + ' fits the conversation', box.right <= li.closest('.ca-thread-sent').getBoundingClientRect().right + 1);
        var cap = li.querySelector('figcaption').getBoundingClientRect();
        check(label + '’s name is no wider than it', cap.width <= Math.max(box.width, 1) + 1, Math.round(cap.width) + ' > ' + Math.round(box.width));
        var dir = li.querySelector('figcaption .ca-md-file-dir');
        check(label + '’s folder is left to its hover', !dir || getComputedStyle(dir).display === 'none');
        var el = li.querySelector('img, video, audio');
        if (el) check(label + ' loads from where it is', /^\.\/files\/%2F/.test(el.getAttribute('src')), el.getAttribute('src'));
      });
      var lines = all('.ca-thread-sent ul:not(.ca-thread-sentmedia) > li');
      lines.forEach(function (li) {
        var ul = li.parentElement.getBoundingClientRect();
        check(li.textContent + ' takes a line of its own', Math.abs(li.getBoundingClientRect().width - ul.width) <= 1);
      });
      note(lis.length + ' shown in place, ' + lines.length + ' as lines');
    })
    .then(done)
    .catch(stopped);
})();
