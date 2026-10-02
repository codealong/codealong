// Checks the images in a conversation's prompts: that each one the page refers to loaded, and
// that the view is still at the end once they have (an image takes its room after the view was
// placed). Run it on a session whose page has a prompt with an image:
//   PREVIEW_TRANSCRIPT=<session key> [PREVIEW_BEFORE=<cursor>] scripts/preview.sh kept '' scripts/preview/prompt-images-check.js
(function () {
  var said = 'waiting';
  setInterval(function () {
    document.title = said;
  }, 50);
  var tries = 0;
  var look = function () {
    var el = document.querySelector('.ca-thread');
    var imgs = el ? Array.prototype.slice.call(el.querySelectorAll('.ca-thread-you .ca-desc-img img')) : [];
    var missing = el ? el.querySelectorAll('.ca-thread-you .ca-desc-noimg').length : 0;
    var loaded = imgs.filter(function (i) { return i.complete && i.naturalWidth > 0; }).length;
    if ((!el || imgs.length + missing === 0 || loaded < imgs.length) && tries++ < 60) return setTimeout(look, 100);
    var gap = el ? Math.round(el.scrollHeight - el.scrollTop - el.clientHeight) : -1;
    var tall = imgs.map(function (i) { return i.offsetHeight; }).join(',');
    said = 'prompt images: ' + loaded + ' of ' + imgs.length + ' loaded (' + tall + ' high), ' + missing + ' not on this Mac, ' + (gap <= 1 ? 'view at the end' : 'view ' + gap + 'px from the end');
  };
  setTimeout(look, 300);
})();
