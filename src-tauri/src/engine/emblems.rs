//! Giving this Mac's sessions and schedules their emblems (emblems.rs), of the kind `[emblems]`
//! has on: asking a decision model for the emoji of each that has none (`look_emblems`, its
//! answer coming back through engine/jevasks.rs `on_jev`), or drawing the newest that has no
//! picture, one at a time, on a background thread (`draw_emblem`, `on_emblem_drawn`).

use super::*;

/// The decision model that may pick an emblem's emoji (emblems.rs `subject`): a session's
/// (decisions.rs `of_session`), or a schedule's, as any of the sessions it may start would be:
/// on whatever account its account, routing group or Auto may land on, in the project it keeps
/// watch over (an orchestrator's; All projects' is every one) or its worktree's
/// (decisions.rs `for_start`).
fn emblem_decider(cfg: &Config, board: &Board, key: &str) -> Option<crate::decisions::Decider> {
    if let Some((lane, s)) = crate::decisions::own_row(&board.lanes, key) {
        return crate::decisions::of_session(cfg, &board.lanes, lane, s);
    }
    let i = board.schedules.iter().find(|i| crate::emblems::schedule_key(&i.schedule.id) == key)?;
    let projects: Vec<Option<&str>> = match &i.schedule.orchestrates {
        Some(o) if o.project.is_some() => vec![o.project.as_deref()],
        Some(_) => std::iter::once(None).chain(cfg.projects.iter().map(|p| Some(p.name.as_str()))).collect(),
        // A folder the board no longer has could be any project's: none is asked.
        None if !i.schedule.run.folder.trim().is_empty() => vec![Some(crate::schedules::place(&i.schedule, board)?.project.as_str())],
        None => vec![None],
    };
    crate::decisions::for_start(cfg, &i.schedule.run.account, &projects)
}

impl Engine {
    /// Gives this Mac's current sessions emblems of the kind that's on (emblems.rs): asks Jev
    /// for the emoji of each that has none, at most a few new questions per board as for
    /// insights, or starts drawing the newest that has no picture, one at a time, the schedules
    /// before them, whose sessions take theirs. Another Mac's sessions come with the emblems that
    /// Mac gave them. True when a session took its schedule's, which the board doesn't show yet.
    pub(super) fn look_emblems(&mut self, board: &Board) -> bool {
        use crate::config::EmblemKind;
        const PER_BOARD: usize = 20;
        let kind = self.cfg.emblems.kind;
        if kind != EmblemKind::Drawn {
            self.set_trouble(Trouble::Emblems, None);
        }
        if kind == EmblemKind::Off || self.passing {
            return false;
        }
        let now = now_ms();
        self.emblems.heard(crate::schedules::machines::their_emblems(&board.machines, now).map(|(_, id, kept)| (id, kept)), crate::schedules::machines::picture_here, now);
        let mut own = crate::schedules::own_sessions(&self.schedules, &self.schedule_runs, board);
        let scheduled: Vec<crate::emblems::Scheduled> = board
            .schedules
            .iter()
            .filter(|i| i.schedule.run.to != crate::schedules::To::Session)
            .map(|i| {
                let sessions = own.remove(&i.schedule.id).unwrap_or_default();
                let asks = crate::schedules::asks_emblem(i, &sessions, board, &self.cfg, now, &crate::schedules::rule::Local);
                crate::emblems::Scheduled { id: &i.schedule.id, project: crate::schedules::place(&i.schedule, board).map_or("", |l| l.project.as_str()), sessions, asks }
            })
            .collect();
        let took = self.emblems.inherit(&scheduled, now);
        // A session's only where what it said may go: to the decision model its project or
        // boundary group lets read it (decisions.rs), and drawn only by an account inside its
        // boundary group (boundary.rs).
        let cfg = &self.cfg;
        let drawer = config::drawing_account(cfg).map(|a| a.name.clone()).unwrap_or_default();
        let drawer_qualifies = config::drawing_account(cfg).is_some_and(|a| crate::signin::current(a, now, crate::signin::Ask::Never).qualifies());
        let may = move |l: &Lane, s: &Session| match kind {
            EmblemKind::Emoji => crate::decisions::of_session(cfg, &board.lanes, l, s).is_some(),
            _ => crate::boundary::group_of(cfg, &s.account).is_none_or(|g| drawer_qualifies && crate::boundary::stays_inside(cfg, g, &drawer)),
        };
        let may = &may;
        let sessions = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(move |l| l.sessions.iter().filter(move |s| s.visitor.is_none() && may(l, s)).map(move |s| (l.project.as_str(), s)));
        let wanting = self.emblems.look(sessions, kind, now);
        if kind == EmblemKind::Emoji {
            let mut asked = 0;
            for key in wanting {
                let id = format!("emblem:{key}");
                if asked >= PER_BOARD {
                    break;
                }
                let Some(subject) = crate::emblems::subject(board, &key).filter(|_| !self.jev_pending.contains(&id)) else { continue };
                let Some(decider) = emblem_decider(&self.cfg, board, &key) else { continue };
                let body = crate::emblems::emoji_request(subject, decider.model());
                self.ask_jev(jev::Feature::Emblems, decider, id, body);
                asked += 1;
            }
        } else if let Some(key) = wanting.into_iter().next().filter(|_| self.emblems.can_draw(now)) {
            self.draw_emblem(board, key);
        }
        self.emblems.save(now);
        took
    }

    /// Starts drawing a session's or a schedule's emblem on the account that draws them, on a
    /// background thread (emblems.rs `draw`).
    fn draw_emblem(&mut self, board: &Board, key: String) {
        let Some(subject) = crate::emblems::subject(board, &key) else { return };
        let Some(account) = config::drawing_account(&self.cfg).cloned() else {
            self.set_trouble(Trouble::Emblems, Some("No account can draw emblems: add a Codex account in Settings › Accounts, or have Jev pick emoji in Settings › Appearance.".into()));
            return;
        };
        let prompt = crate::emblems::draw_prompt(subject, &self.emblems.taken(&key, config::EmblemKind::Drawn), &self.cfg.emblems.prompt);
        self.emblems.drawing = Some(key.clone());
        let tx = self.tx.clone();
        // `on_emblem_drawn` hears that it's over however it ends, so the next one can start.
        throttle::background(
            "codealong-emblem",
            move || crate::emblems::draw(&account, &crate::emblems::drawing_dir(), &prompt).map_err(crate::util::words),
            move |drawn| {
                let _ = tx.send(Msg::EmblemDrawn(key, drawn.unwrap_or_else(|| Err("The drawing stopped partway.".into()))));
            },
        );
    }

    pub(super) fn on_emblem_drawn(&mut self, key: &str, drawn: std::result::Result<(String, String), String>) {
        let now = now_ms();
        match drawn {
            Ok((picture, name)) => {
                self.emblems.drawn(key, picture, name, now);
                self.set_trouble(Trouble::Emblems, None);
            }
            Err(why) => {
                self.emblems.draw_failed(key, now);
                self.set_trouble(Trouble::Emblems, Some(why));
            }
        }
        self.mark_dirty("an emblem drawn");
        self.emblems.save(now);
    }

    /// The emoji Jev ranked for a session's emblem.
    pub(super) fn on_emblem(&mut self, key: &str, answer: std::result::Result<serde_json::Value, String>) {
        let now = now_ms();
        match answer {
            Ok(response) if self.emblems.answered_emoji(key, &jev::choice(&response, "emoji"), now) => {
                self.emblems.save(now);
                self.mark_dirty("an emblem's emoji");
            }
            Ok(_) => {}
            Err(_) => self.emblems.failed(key, now),
        }
    }
}
