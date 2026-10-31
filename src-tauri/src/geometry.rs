//! Where things are on the screen: rectangles as AppKit gives them, rectangles as a window's page
//! measures them, and the screens they're on. Two ways of counting meet in the windows that float
//! (the bar, its peek, the cards on the desktop): AppKit's y grows up from the bottom of the main
//! screen, and a page's grows down from the top of its window. `Rect` is the first and `PageRect`
//! the second, so the flip between them is written once, here.

use serde::{Deserialize, Serialize};

/// Room kept at the sides of the screen.
pub(crate) const EDGE: f64 = 8.0;
/// Space between a newly placed bar or card and the menu bar.
pub(crate) const TOP_GAP: f64 = 6.0;
/// The room the floating bar's page leaves around its card, for its shadow (Bar.tsx `MARGIN`),
/// and a desktop card's page around its own (DesktopPin.tsx's `MARGIN`): the card itself is this
/// far inside its window.
pub(crate) const CARD_MARGIN: f64 = 12.0;

/// A rectangle in screen points as AppKit gives them: the origin at the bottom left of the main
/// screen, y growing up.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub(crate) fn top(&self) -> f64 {
        self.y + self.h
    }
    pub(crate) fn right(&self) -> f64 {
        self.x + self.w
    }
    pub(crate) fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub(crate) fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.top()
    }

    /// Whether all of another rectangle is inside this one, touching its edges or not.
    pub(crate) fn holds(&self, r: Rect) -> bool {
        r.x >= self.x && r.right() <= self.right() && r.y >= self.y && r.top() <= self.top()
    }

    /// The rectangle of this size whose top left corner is at `x`, `top`: how a window that
    /// keeps its top edge is placed, since AppKit places it by its bottom.
    pub(crate) fn from_top_left(x: f64, top: f64, w: f64, h: f64) -> Rect {
        Rect { x, y: top - h, w, h }
    }

    /// This rectangle with `by` more on every side.
    pub(crate) fn outset(&self, by: f64) -> Rect {
        Rect { x: self.x - by, y: self.y - by, w: self.w + 2.0 * by, h: self.h + 2.0 * by }
    }

    /// This rectangle with `by` less on every side.
    pub(crate) fn inset(&self, by: f64) -> Rect {
        Rect { x: self.x + by, y: self.y + by, w: self.w - 2.0 * by, h: self.h - 2.0 * by }
    }

    /// A point on the screen as the page of a window with this frame counts it: from the
    /// window's top left, y growing down.
    pub(crate) fn page_point(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x - self.x, self.top() - y)
    }
}

/// A rectangle as a window's page measures it (CSS pixels, which are points): from the top left
/// of its window, the host, y growing down. A pill in the bar, the shape the docked bar shows, a
/// card in the window of lines between cards.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl PageRect {
    /// Where it is on the screen, given its window's frame.
    pub(crate) fn to_screen(&self, host: Rect) -> Rect {
        Rect { x: host.x + self.x, y: host.top() - self.y - self.h, w: self.w, h: self.h }
    }

    /// A rectangle on the screen as the page of the window at `host` counts it.
    pub(crate) fn from_screen(r: Rect, host: Rect) -> PageRect {
        PageRect { x: r.x - host.x, y: host.top() - r.top(), w: r.w, h: r.h }
    }

    /// Whether a point the page counts the same way (`Rect::page_point`) is on it.
    pub(crate) fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// A screen: all of it, and the part the menu bar and Dock leave free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub frame: Rect,
    pub visible: Rect,
}

/// Where a window goes at a new size. It keeps its top edge and the side it's nearest: one in
/// the left third of the screen grows to the right, one in the right third grows to the left, and
/// one in the middle stays centered. It stays on the screen. The floating bar and the cards on
/// the desktop are sized this way as their pages measure themselves.
pub fn place(current: Rect, w: f64, h: f64, screen: Rect) -> Rect {
    let (cx, _) = current.center();
    let third = (cx - screen.x) / screen.w;
    let x = if third < 1.0 / 3.0 {
        current.x
    } else if third > 2.0 / 3.0 {
        current.right() - w
    } else {
        cx - w / 2.0
    };
    clamp(Rect::from_top_left(x, current.top(), w, h), screen)
}

/// Moves the rectangle onto the screen, narrowing it only when it's wider than the screen.
pub(crate) fn clamp(r: Rect, screen: Rect) -> Rect {
    let w = r.w.min(screen.w - 2.0 * EDGE).max(1.0);
    let h = r.h.min(screen.h).max(1.0);
    let x = r.x.min(screen.right() - EDGE - w).max(screen.x + EDGE);
    let y = r.y.min(screen.top() - h).max(screen.y);
    Rect { x, y, w, h }
}

/// The screen a point is on, if it's on one.
pub(crate) fn screen_holding(p: (f64, f64), screens: &[Screen]) -> Option<Screen> {
    screens.iter().find(|s| s.frame.contains(p)).copied()
}

/// The screen a point is on, else the first: the one with the menu bar.
pub(crate) fn screen_at(p: (f64, f64), screens: &[Screen]) -> Option<Screen> {
    screen_holding(p, screens).or(screens.first().copied())
}

/// The usable area of the screen a rectangle is on: the one holding its center, else the first.
pub(crate) fn screen_for(r: Rect, screens: &[Screen]) -> Option<Rect> {
    screen_at(r.center(), screens).map(|s| s.visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1500.0, h: 900.0 };

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    fn page(x: f64, y: f64, w: f64, h: f64) -> PageRect {
        PageRect { x, y, w, h }
    }

    #[test]
    fn grows_from_the_side_it_is_nearest() {
        // Middle third: stays centered.
        assert_eq!(place(r(650.0, 800.0, 200.0, 40.0), 400.0, 40.0, SCREEN), r(550.0, 800.0, 400.0, 40.0));
        // Left third: keeps its left edge.
        assert_eq!(place(r(100.0, 800.0, 200.0, 40.0), 400.0, 40.0, SCREEN), r(100.0, 800.0, 400.0, 40.0));
        // Right third: keeps its right edge.
        assert_eq!(place(r(1200.0, 800.0, 200.0, 40.0), 400.0, 40.0, SCREEN), r(1000.0, 800.0, 400.0, 40.0));
    }

    #[test]
    fn keeps_its_top_edge_and_stays_on_screen() {
        assert_eq!(place(r(650.0, 800.0, 200.0, 40.0), 200.0, 50.0, SCREEN), r(650.0, 790.0, 200.0, 50.0));
        // Near the left edge a centered bar would spill off; it's pushed back on.
        assert_eq!(place(r(10.0, 100.0, 40.0, 40.0), 300.0, 40.0, SCREEN).x, 10.0);
        assert_eq!(place(r(600.0, 100.0, 300.0, 40.0), 2000.0, 40.0, SCREEN), r(8.0, 100.0, 1484.0, 40.0));
        // Dragged partly above the usable area, it comes back under it.
        assert_eq!(place(r(600.0, 880.0, 300.0, 40.0), 300.0, 40.0, SCREEN).y, 860.0);
    }

    #[test]
    fn finds_the_pill_on_the_screen() {
        // A bar at the top of the screen; the page measures from its window's top left.
        let bar = r(500.0, 800.0, 600.0, 56.0);
        assert_eq!(page(100.0, 17.0, 120.0, 22.0).to_screen(bar), r(600.0, 817.0, 120.0, 22.0));
    }

    #[test]
    fn a_page_s_rectangle_goes_to_the_screen_and_back() {
        // A window on a display up and to the left of the main one, as AppKit places it.
        let host = r(-1200.0, 1117.0, 735.0, 78.0);
        let shape = page(225.0, 0.0, 285.0, 32.0);
        let on_screen = shape.to_screen(host);
        assert_eq!(on_screen, r(-975.0, 1163.0, 285.0, 32.0));
        assert_eq!(on_screen.top(), host.top(), "at the top of its window");
        assert_eq!(PageRect::from_screen(on_screen, host), shape);
        // The whole window is its own page's rectangle from the corner.
        assert_eq!(PageRect::from_screen(host, host), page(0.0, 0.0, 735.0, 78.0));
    }

    #[test]
    fn a_point_on_the_screen_is_counted_from_the_window_s_top_left() {
        let host = r(496.0, 1039.0, 735.0, 78.0);
        assert_eq!(host.page_point((496.0, 1117.0)), (0.0, 0.0));
        assert_eq!(host.page_point((721.0, 1107.0)), (225.0, 10.0));
        let shape = page(225.0, 0.0, 285.0, 32.0);
        assert!(shape.contains(host.page_point((721.0, 1117.0))), "its top left corner");
        assert!(!shape.contains(host.page_point((721.0, 1084.0))), "just under it");
        // The same point, asked the other way round.
        assert!(shape.to_screen(host).contains((721.0, 1117.0)));
        assert!(!shape.to_screen(host).contains((721.0, 1084.0)));
    }

    #[test]
    fn a_window_hangs_from_its_top_left_corner() {
        let f = Rect::from_top_left(100.0, 600.0, 300.0, 80.0);
        assert_eq!(f, r(100.0, 520.0, 300.0, 80.0));
        assert_eq!((f.x, f.top()), (100.0, 600.0));
    }

    #[test]
    fn a_margin_comes_off_every_side_and_goes_back_on() {
        let window = r(100.0, 200.0, 304.0, 80.0);
        let card = window.inset(CARD_MARGIN);
        assert_eq!(card, r(112.0, 212.0, 280.0, 56.0));
        assert_eq!(card.outset(CARD_MARGIN), window);
    }

    #[test]
    fn finds_the_screen_a_point_or_a_window_is_on() {
        let main = Screen { frame: SCREEN, visible: r(0.0, 0.0, 1500.0, 875.0) };
        let left = Screen { frame: r(-1920.0, 0.0, 1920.0, 1080.0), visible: r(-1920.0, 0.0, 1920.0, 1055.0) };
        let screens = [main, left];
        assert_eq!(screen_holding((-500.0, 500.0), &screens), Some(left));
        assert_eq!(screen_holding((5000.0, 500.0), &screens), None);
        // Off every screen, it's the one with the menu bar.
        assert_eq!(screen_at((5000.0, 500.0), &screens), Some(main));
        assert_eq!(screen_at((-500.0, 500.0), &screens), Some(left));
        assert_eq!(screen_at((0.0, 0.0), &[]), None);
        // A window is on the screen that holds its middle, and gets the part it can use.
        assert_eq!(screen_for(r(-200.0, 100.0, 300.0, 80.0), &screens), Some(left.visible));
        assert_eq!(screen_for(r(-100.0, 100.0, 300.0, 80.0), &screens), Some(main.visible));
        assert_eq!(screen_for(r(9000.0, 100.0, 300.0, 80.0), &screens), Some(main.visible));
    }
}
