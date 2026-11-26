//! Experimental per-session AI backgrounds (backgrounds.rs): when `[experimental] session_themes`
//! and `[backgrounds] enabled` are both on, draws a bespoke vertical fine-art background for each
//! recent top-level session on a background thread (one at a time), skipping helpers, automatic PR
//! review helpers, and headless runs.

use super::*;

impl Engine {
    /// Looks over this Mac's recent top-level sessions and starts drawing the newest one missing an
    /// AI background, one at a time, when both `[experimental] session_themes` and
    /// `[backgrounds] enabled` are turned on.
    pub(super) fn look_backgrounds(&mut self, board: &Board) {
        let active = self.cfg.experimental.session_themes && self.cfg.backgrounds.enabled;
        if !active {
            self.set_trouble(Trouble::Backgrounds, None);
            return;
        }
        if self.passing {
            return;
        }
        let now = now_ms();
        let cfg = &self.cfg;
        let drawer = crate::backgrounds::drawing_account(cfg);
        let drawer_name = drawer.map(|a| a.name.clone()).unwrap_or_default();
        let drawer_qualifies = drawer.is_some_and(|a| crate::signin::current(a, now, crate::signin::Ask::Never).qualifies());
        let may = move |s: &Session| crate::boundary::group_of(cfg, &s.account).is_none_or(|g| drawer_qualifies && crate::boundary::stays_inside(cfg, g, &drawer_name));
        let may = &may;
        let sessions = board
            .lanes
            .iter()
            .filter(|l| l.device.is_none())
            .flat_map(move |l| l.sessions.iter().filter(move |s| s.visitor.is_none() && may(s)).map(move |s| (l.project.as_str(), s)));
        let wanting = self.backgrounds.look(sessions, now);
        if let Some(key) = wanting.into_iter().next().filter(|_| self.backgrounds.can_draw(now)) {
            self.draw_background(board, key);
        }
        self.backgrounds.save();
    }

    /// Starts drawing `key`'s AI background on the configured drawing account (Codex or
    /// Antigravity), off the engine thread.
    fn draw_background(&mut self, board: &Board, key: String) {
        let Some((lane, session)) = board.session(&key) else { return };
        let Some(account) = crate::backgrounds::drawing_account(&self.cfg).cloned() else {
            self.set_trouble(
                Trouble::Backgrounds,
                Some("No account can draw backgrounds: add a Codex or Antigravity account in Settings › Accounts.".into()),
            );
            return;
        };
        let team = crate::backgrounds::team_for(board, &lane.repo);
        let style = crate::backgrounds::effective_prompt(&self.cfg, team, &lane.project);
        let prompt = crate::backgrounds::draw_prompt(session, &self.backgrounds.taken(&key), style);
        self.backgrounds.drawing = Some(key.clone());
        let tx = self.tx.clone();
        throttle::background(
            "codealong-background",
            move || crate::backgrounds::draw(&account, &crate::backgrounds::drawing_dir(), &prompt).map_err(crate::util::words),
            move |drawn| {
                let _ = tx.send(Msg::BackgroundDrawn(key, drawn.unwrap_or_else(|| Err("The background drawing stopped partway.".into()))));
            },
        );
    }

    pub(super) fn on_background_drawn(&mut self, key: &str, drawn: std::result::Result<SessionBackdrop, String>) {
        let now = now_ms();
        match drawn {
            Ok(bg) => {
                self.backgrounds.drawn(key, bg, now);
                self.set_trouble(Trouble::Backgrounds, None);
            }
            Err(why) => {
                self.backgrounds.draw_failed(key, now);
                self.set_trouble(Trouble::Backgrounds, Some(why));
            }
        }
        self.backgrounds.save();
        self.mark_dirty("background drawn");
    }

    /// Drops `key`'s kept AI background so a fresh drawing starts immediately.
    pub(super) fn redraw_background(&mut self, key: &str) {
        self.backgrounds.reset(key);
        self.backgrounds.save();
        self.set_trouble(Trouble::Backgrounds, None);
        self.publish_for("redraw background");
    }
}
