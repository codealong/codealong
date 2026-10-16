// Where the pointer is, as a page has it and as macOS has it, for finding why a list's hover lands
// rows away from the pointer: reported 2026-10-12 as two rows below it in Sessions, staying there
// with the pointer still, and a click there selecting that row. No test window has shown it.
//
// WebKit draws a page from the app's side, where a list's scroll runs on its own as you scroll,
// and hit-tests it in the page's process, with the page's scroll positions and the pointer the
// app sent it; what's drawn under the pointer and what the page finds there can only differ when
// one of those two disagrees with the app's side. So while `[debug] perf_log` is on, a board
// window's page reports each time the pointer comes to rest over it (src/pointerProbe.ts): where
// it has the pointer, what it finds there, and how far each list under it is scrolled. This
// compares that with where macOS has the pointer and where WebKit's drawing side has each list
// scrolled (its scrolling tree, from WKWebView's testing SPI `_scrollingTreeAsText`), and writes
// a line to the perf log when they disagree, and one when the first report agrees, to say it runs.
// What's read of the window is os::windows's (`read_page`); WebKitGTK has no scrolling tree to
// read, so on Linux nothing is compared.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::WebviewWindow;

use crate::geometry::Rect;
use crate::{os, perf};

/// A list on the page under the pointer: what it is and how far it's scrolled, in CSS pixels.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageScroller {
    /// Its first class name, to say which list it is.
    pub name: String,
    pub top: f64,
    /// Its content's height (`scrollHeight`) and the box it shows it in.
    pub height: f64,
    pub view_w: f64,
    pub view_h: f64,
    /// Its last scroll positions, newest last, each with how long ago.
    #[serde(default)]
    pub recent: Vec<String>,
}

/// What a page reports when the pointer comes to rest over it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageProbe {
    /// Where the page has the pointer: its last pointer event's, in CSS pixels.
    pub x: f64,
    pub y: f64,
    /// How long ago that event was made, by macOS's clock, in ms: a page behind on its events
    /// has the pointer where it was.
    pub age: f64,
    /// The page's zoom: points per CSS pixel.
    pub zoom: f64,
    /// What the page finds at the pointer: a row's name, or the element.
    pub under: String,
    /// The lists that hold it, innermost first.
    pub scrollers: Vec<PageScroller>,
}

/// A list WebKit scrolls on the app's side: the box it shows, its content, and where it's
/// scrolled to there, in the page's layout units (CSS pixels times the zoom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollNode {
    pub area: (f64, f64),
    pub content: (f64, f64),
    pub at: (f64, f64),
}

/// The scrolled lists in WebKit's scrolling tree as `_scrollingTreeAsText` writes it: each
/// `(overflow scrolling node` with its `(scrollable area size width=… height=…)`, `(total content
/// size …)` and `(scroll position (x,y))`, where it's drawn, which it leaves out at 0,0. The
/// `(last committed scroll position (x,y))` beside it is the one the page last sent, not where
/// it's drawn.
pub fn scroll_nodes(tree: &str) -> Vec<ScrollNode> {
    tree.split("(overflow scrolling node")
        .skip(1)
        .filter_map(|node| {
            // A node's own lines come before the first node inside it (a sticky node, say).
            let own = node.split(" node\n").next().unwrap_or(node);
            let size = |name: &str| -> Option<(f64, f64)> {
                let rest = &own[own.find(name)? + name.len()..];
                let w = number_after(rest, "width=")?;
                let h = number_after(rest, "height=")?;
                Some((w, h))
            };
            let point = |name: &str| -> Option<(f64, f64)> {
                let rest = &own[own.find(name)? + name.len()..];
                let inside = rest.trim_start().strip_prefix('(')?;
                let (x, rest) = inside.split_once(',')?;
                let y = rest.split(')').next()?;
                Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
            };
            // "(scroll position" isn't in "(last committed scroll position", nor in a proxy's
            // "(related overflow scrolling node scroll position". A node without it is a
            // scrolling node all the same, drawn at the top.
            own.contains("(last committed scroll position")
                .then(|| Some(ScrollNode { area: size("(scrollable area size")?, content: size("(total content size")?, at: point("(scroll position").unwrap_or((0.0, 0.0)) }))
                .flatten()
        })
        .collect()
}

fn number_after(s: &str, name: &str) -> Option<f64> {
    let rest = &s[s.find(name)? + name.len()..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// How far apart the page's pointer may be from macOS's, in CSS pixels, and still agree: the
/// page rounds, and a pointer can creep a little after it rests.
const POINTER_SLACK: f64 = 4.0;
/// How far apart a list's scroll may be on the two sides and still agree.
const SCROLL_SLACK: f64 = 2.0;

/// What a report says against macOS's pointer (`pointer`, in the page's CSS pixels from its top
/// left, if it's known) and WebKit's drawing side (`nodes`): the line to log when the two
/// disagree, or `None` when they agree. A list is matched to its node by its box and content.
pub fn disagreement(probe: &PageProbe, pointer: Option<(f64, f64)>, nodes: &[ScrollNode]) -> Option<String> {
    let z = if probe.zoom > 0.0 { probe.zoom } else { 1.0 };
    let mut off = Vec::new();
    if let Some((x, y)) = pointer {
        let (dx, dy) = (x - probe.x, y - probe.y);
        if dx.abs() > POINTER_SLACK || dy.abs() > POINTER_SLACK {
            off.push(format!("the pointer is at {x:.0},{y:.0} by macOS, {dx:+.0},{dy:+.0} from the page's"));
        }
    }
    let near = |a: f64, b: f64| (a - b).abs() <= 1.5;
    for s in &probe.scrollers {
        let node = nodes.iter().find(|n| near(n.area.0, s.view_w * z) && near(n.area.1, s.view_h * z) && near(n.content.1, s.height * z));
        match node {
            Some(n) if (n.at.1 - s.top * z).abs() > SCROLL_SLACK => {
                off.push(format!("{} is drawn scrolled to {:.0}, {:+.0} from the page's {:.0}", s.name, n.at.1 / z, n.at.1 / z - s.top, s.top))
            }
            Some(_) => {}
            None => off.push(format!("{} has no node on the drawing side ({:.0}x{:.0}, content {:.0})", s.name, s.view_w, s.view_h, s.height)),
        }
    }
    if off.is_empty() {
        return None;
    }
    Some(format!("pointer off: {}; {}", off.join("; "), said(probe)))
}

/// What the page reported, for a log line.
fn said(probe: &PageProbe) -> String {
    let lists: Vec<String> = probe
        .scrollers
        .iter()
        .map(|s| {
            let recent = if s.recent.is_empty() { String::new() } else { format!(", lately {}", s.recent.join(" ")) };
            format!("{} at {:.0} of {:.0} in {:.0}x{:.0}{recent}", s.name, s.top, s.height, s.view_w, s.view_h)
        })
        .collect();
    format!("the page has the pointer at {:.0},{:.0} ({:.0} ms old) on {:?}, zoom {}; lists: {}", probe.x, probe.y, probe.age, probe.under, probe.zoom, if lists.is_empty() { "none".to_string() } else { lists.join("; ") })
}

/// Where macOS has the pointer, `p` in screen points, as a page in `host` (its box on the screen)
/// at `zoom` counts it, in its CSS pixels from its top left: none when it's off the page, where
/// a report the page made before it left would compare with nothing.
pub fn page_pointer(host: Rect, p: (f64, f64), zoom: f64) -> Option<(f64, f64)> {
    if !host.contains(p) {
        return None;
    }
    let z = if zoom > 0.0 { zoom } else { 1.0 };
    let (x, y) = host.page_point(p);
    Some((x / z, y / z))
}

/// The first report agreeing is logged, to say the probe runs; a disagreement at most this often.
static SAID_AGREE: AtomicBool = AtomicBool::new(false);
static LAST_OFF: Mutex<Option<Instant>> = Mutex::new(None);
const OFF_EVERY: Duration = Duration::from_secs(2);

/// Compares a page's report with the OS and its webview's drawing side, on the main thread, and
/// logs what disagrees.
pub fn compare(w: &WebviewWindow, probe: PageProbe) {
    if !perf::enabled() {
        return;
    }
    let label = w.label().to_string();
    os::windows::read_page(w, move |page| {
        let Some(pointer) = page_pointer(page.host, page.pointer, probe.zoom) else { return };
        let nodes = page.scrolling_tree.as_deref().map(scroll_nodes).unwrap_or_default();
        // A page whose app isn't frontmost has its drawing and timers slowed, which could leave
        // it behind the app's side.
        let line = match disagreement(&probe, Some(pointer), &nodes) {
            Some(off) => {
                let mut last = LAST_OFF.lock().unwrap_or_else(|e| e.into_inner());
                if last.is_some_and(|t| t.elapsed() < OFF_EVERY) {
                    return;
                }
                *last = Some(Instant::now());
                Some(format!("{off}; the page's box {}, the window {}; Codealong active {}, key window {}", shown(page.host), shown(page.window), yes(page.active), yes(page.key)))
            }
            None if !SAID_AGREE.swap(true, Ordering::Relaxed) => Some(format!("pointer probe on, and agrees: {}", said(&probe))),
            None => None,
        };
        if let Some(line) = line {
            perf::log(&format!("ui[{label}] {line}"));
        }
    });
}

fn shown(r: Rect) -> String {
    format!("{:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.w, r.h)
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "
(scrolling tree
  (frame scrolling node
    (scrollable area size width=1440 height=900)
    (total content size width=1440 height=900)
    (last committed scroll position (0,0))
    (overflow scrolling node
      (scrollable area size width=639 height=809)
      (total content size width=639 height=4971)
      (last committed scroll position (0,50))
      (scroll position (0,90))
      (scrollable area parameters
        (allows vertical scrolling 1)))
      (sticky node
        (layer top left (0,-2))))
    (overflow scrolling node
      (scrollable area size width=263 height=761)
      (total content size width=263 height=1200)
      (last committed scroll position (0,120))
      (scrollable area parameters
        (allows vertical scrolling 1)))
    (overflow scroll proxy node
      (related overflow scrolling node scroll position (0,90)))))
";

    fn probe(top: f64) -> PageProbe {
        PageProbe {
            x: 300.0,
            y: 400.0,
            age: 410.0,
            zoom: 1.0,
            under: "row".into(),
            scrollers: vec![PageScroller { name: "ca-board".into(), top, height: 4971.0, view_w: 639.0, view_h: 809.0, recent: vec![] }],
        }
    }

    #[test]
    fn reads_each_scrolled_list_where_its_drawn_not_where_the_page_last_said() {
        let nodes = scroll_nodes(TREE);
        // The second says only where the page last had it: it's drawn at the top.
        assert_eq!(nodes, vec![
            ScrollNode { area: (639.0, 809.0), content: (639.0, 4971.0), at: (0.0, 90.0) },
            ScrollNode { area: (263.0, 761.0), content: (263.0, 1200.0), at: (0.0, 0.0) },
        ]);
    }

    #[test]
    fn says_a_list_drawn_at_the_top_that_the_page_has_scrolled() {
        let nodes = scroll_nodes(TREE);
        let mut p = probe(90.0);
        p.scrollers.push(PageScroller { name: "ca-side".into(), top: 120.0, height: 1200.0, view_w: 263.0, view_h: 761.0, recent: vec![] });
        let line = disagreement(&p, None, &nodes).unwrap();
        assert!(line.contains("ca-side is drawn scrolled to 0, -120 from the page's 120"), "{line}");
        assert!(!line.contains("ca-board is drawn"), "{line}");
    }

    #[test]
    fn agrees_when_the_pointer_and_the_lists_match() {
        let nodes = scroll_nodes(TREE);
        assert_eq!(disagreement(&probe(90.0), Some((302.0, 398.0)), &nodes), None);
    }

    #[test]
    fn says_a_list_drawn_scrolled_elsewhere_and_by_how_much() {
        let nodes = scroll_nodes(TREE);
        let line = disagreement(&probe(156.0), Some((300.0, 400.0)), &nodes).unwrap();
        assert!(line.contains("ca-board is drawn scrolled to 90, -66 from the page's 156"), "{line}");
    }

    #[test]
    fn says_a_pointer_the_page_has_elsewhere() {
        let nodes = scroll_nodes(TREE);
        let line = disagreement(&probe(90.0), Some((300.0, 334.0)), &nodes).unwrap();
        assert!(line.contains("+0,-66 from the page's"), "{line}");
    }

    #[test]
    fn counts_the_pointer_from_the_pages_top_left_and_not_off_it() {
        // A page 1000 by 800 whose bottom left is at 100,50 on the screen.
        let host = Rect { x: 100.0, y: 50.0, w: 1000.0, h: 800.0 };
        assert_eq!(page_pointer(host, (400.0, 650.0), 1.0), Some((300.0, 200.0)));
        assert_eq!(page_pointer(host, (400.0, 650.0), 1.25), Some((240.0, 160.0)));
        // Left the window after the page's last pointer event: nothing to compare with.
        assert_eq!(page_pointer(host, (1200.0, 650.0), 1.0), None);
        assert_eq!(page_pointer(host, (400.0, 900.0), 1.0), None);
    }

    #[test]
    fn compares_a_zoomed_page_in_its_own_pixels() {
        let nodes = vec![ScrollNode { area: (702.9, 889.9), content: (702.9, 5468.1), at: (0.0, 99.0) }];
        let mut p = probe(90.0);
        p.zoom = 1.1;
        assert_eq!(disagreement(&p, None, &nodes), None);
    }
}
