// Checks the images in a conversation's prompts: that each one the page refers to loaded, and
// that the view is still at the end once they have (an image takes its room after the view was
// placed). Run it on a session whose page has a prompt with an image:
//   PREVIEW_TRANSCRIPT=<session key> [PREVIEW_BEFORE=<cursor>] scripts/preview.sh kept '' scripts/preview/prompt-images-check.js
(function () {
  var thread = function () { return q('.ca-thread'); };
  var imgs = function () { return all('.ca-thread-you .ca-desc-img img', thread()); };
  var missing = function () { return all('.ca-thread-you .ca-desc-noimg', thread()).length; };
  var loaded = function () { return imgs().filter(function (i) { return i.complete && i.naturalWidth > 0; }).length; };
  say('prompt images: waiting');
  until(function () { return thread() && imgs().length + missing() > 0; }, 6000, 'a prompt with an image in the conversation')
    .then(function () { return until(function () { return loaded() === imgs().length; }, 6000).catch(function () {}); })
    .then(function () {
      var el = thread();
      var gap = Math.round(el.scrollHeight - el.scrollTop - el.clientHeight);
      check('each image on this Mac loads', loaded() === imgs().length, loaded() + ' of ' + imgs().length + ' (' + imgs().map(function (i) { return i.offsetHeight; }).join(',') + ' high)');
      check('the view is still at the end', gap <= 1, gap + 'px from the end');
      if (missing() > 0) note(missing() + ' not on this Mac');
    })
    .then(done)
    .catch(stopped);
})();
