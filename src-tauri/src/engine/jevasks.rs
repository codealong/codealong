//! What the engine asks the decision models (decisions.rs), and what it does with their answers:
//! which todo a new session is for and whether a todo's work looks done (todojev.rs), and a read
//! of each session's moment (insights.rs). It looks for what to ask each time `publish` has
//! composed a board (`jev_look`), before that board is compared with the last one: whether or not
//! it then goes out. A question goes to a thread of its own under an id (`ask_jev`) and its
//! answer comes back to `on_jev`, which goes by the id's first part; an emblem's emoji is asked
//! the same way (engine/emblems.rs).

use super::*;

/// After a question to Jev fails (no key, TypeSafe down), how long before more are asked.
const JEV_RETRY: Duration = Duration::from_secs(5 * 60);

impl Engine {
    /// Asks a decision model in the background for a feature: `decider`, the one that may read
    /// what the body says (decisions.rs). The answer comes back to `on_jev` under the same id; an
    /// id already waiting for its answer isn't asked again.
    pub(super) fn ask_jev(&mut self, feature: jev::Feature, decider: crate::decisions::Decider, id: String, body: serde_json::Value) {
        self.ask_jev_filled(feature, decider, id, body, None);
    }

    /// Asks with a body the worker finishes on its own thread first (`decisions::Job::fill`).
    fn ask_jev_filled(&mut self, feature: jev::Feature, decider: crate::decisions::Decider, id: String, body: serde_json::Value, fill: Option<Box<dyn FnOnce(&mut serde_json::Value) + Send>>) {
        if self.jev_pending.insert(id.clone()) {
            let fingerprint = crate::decisions::fingerprint(&self.cfg);
            let _ = self.jev_jobs.send(crate::decisions::Job { id, feature, decider, fingerprint, body, fill });
        }
    }

    /// An answer from Jev. The id's first part, up to `:`, says which feature asked.
    pub(super) fn on_jev(&mut self, id: String, answer: std::result::Result<serde_json::Value, String>) {
        self.jev_pending.remove(&id);
        let (feature, rest) = id.split_once(':').unwrap_or((id.as_str(), ""));
        match feature {
            "todolink" => self.on_todo_link(rest.to_string(), answer),
            "tododone" => self.on_todo_done(rest.to_string(), answer),
            "insight" => self.on_insight(rest.to_string(), answer),
            "emblem" => self.on_emblem(rest, answer),
            _ => {
                let _ = answer;
            }
        }
    }

    /// Jev's answer on which todo a new session is for: linked when it's sure.
    fn on_todo_link(&mut self, key: String, answer: std::result::Result<serde_json::Value, String>) {
        let Some(ids) = self.jev_link_asking.remove(&key) else { return };
        let Ok(response) = answer else {
            // Asked again on a later look, unless it's too old by then.
            self.jev_todos.asked.remove(&key);
            self.jev_todos_failed = Some(Instant::now());
            return;
        };
        self.jev_todos.save();
        let Some(id) = todojev::link_pick(&response, &ids) else { return };
        let lanes = self.lane_index();
        let Ok(_lock) = self.lock_todos() else { return };
        self.reload_todos();
        if self.todos_unreadable || self.todos.todos.iter().any(|t| t.sessions.contains(&key)) {
            return;
        }
        if !self.todos.todos.iter().any(|t| t.id == id && todos::is_open(t)) {
            return;
        }
        let op = TodoOp::Link { id, worktree: None, session: Some(key), linked: true };
        if self.todos.apply(op, &lanes, now_ms()).unwrap_or(false) {
            self.save_todos();
        }
    }

    /// Jev's answer on whether a todo's work looks done.
    fn on_todo_done(&mut self, id: String, answer: std::result::Result<serde_json::Value, String>) {
        let Some((sig, why)) = self.jev_done_asking.remove(&id) else { return };
        let Ok(response) = answer else {
            self.jev_todos_failed = Some(Instant::now());
            return;
        };
        let Some(p) = jev::noul(&response, "done") else { return };
        self.jev_todos.done.insert(id, todojev::DoneAnswer { sig, p, why });
        self.jev_todos.save();
        self.mark_dirty("Jev on a todo");
    }

    /// Says a todo doesn't look done after all, until something linked to it moves on.
    pub(super) fn dismiss_todo_hint(&mut self, id: String) {
        if let Some(a) = self.jev_todos.done.get(&id) {
            self.jev_todos.dismissed.insert(id, a.sig.clone());
            self.jev_todos.save();
            self.mark_dirty("a todo hint dismissed");
        }
    }

    /// New sessions to link, and todos whose linked work moved on, go to Jev.
    fn jev_look_todos(&mut self, board: &Board) {
        if !self.jev_todos_loaded {
            self.jev_todos = todojev::Memory::load();
            self.jev_todos_loaded = true;
        }
        if self.jev_todos_failed.is_some_and(|t| t.elapsed() < JEV_RETRY) {
            return;
        }
        let now = now_ms();
        let cfg = &self.cfg;
        // Each question goes to the model that may read its subject (decisions.rs): a new
        // session's, whose project's todos it's shown whole only when that model may read them
        // too; a todo's project's, and not at all while a session linked to it keeps its words
        // from that model.
        let asks: Vec<(String, crate::decisions::Decider, serde_json::Value, Vec<String>)> = todojev::link_candidates(board, &self.todos.todos, &self.jev_todos.asked, now)
            .into_iter()
            .filter_map(|(lane, s)| {
                let decider = crate::decisions::of_session(cfg, &board.lanes, lane, s)?;
                let todos_said = decider.carries(crate::decisions::for_project(cfg, Some(&lane.project)).as_ref());
                let (body, ids) = todojev::link_request(decider.model(), board, lane, s, &self.todos.todos, todos_said);
                Some((s.key.clone(), decider, body, ids))
            })
            .collect();
        let done: Vec<(String, crate::decisions::Decider, todojev::DoneInput)> = self
            .todos
            .todos
            .iter()
            .filter_map(|t| {
                let decider = crate::decisions::for_todo(cfg, &board.lanes, t)?;
                let kept = t.sessions.iter().any(|k| {
                    let mut rows = crate::decisions::rows(&board.lanes, k).peekable();
                    match rows.peek() {
                        Some(_) => rows.any(|(l, s)| !decider.carries(crate::decisions::of_session(cfg, &board.lanes, l, s).as_ref())),
                        // Off the board: by its account (the key's second part), in the todo's project.
                        None => crate::boundary::account_of_key(k).is_some_and(|a| !decider.carries(crate::decisions::for_session(cfg, a, Some(t.project.as_str()).filter(|p| !p.is_empty())).as_ref())),
                    }
                });
                if kept {
                    return None;
                }
                let input = todojev::done_input(board, t)?;
                Some((t.id.clone(), decider, input))
            })
            // One question per todo at a time; a newer state is asked about once it's answered.
            .filter(|(id, _, i)| self.jev_todos.done.get(id).is_none_or(|a| a.sig != i.sig) && !self.jev_done_asking.contains_key(id))
            .collect();
        for (key, decider, body, ids) in asks {
            self.jev_todos.asked.insert(key.clone());
            self.jev_link_asking.insert(key.clone(), ids);
            self.ask_jev(jev::Feature::Todos, decider, format!("todolink:{key}"), body);
        }
        for (id, decider, input) in done {
            self.jev_done_asking.insert(id.clone(), (input.sig.clone(), input.why.clone()));
            let body = todojev::done_request(decider.model(), &input);
            self.ask_jev(jev::Feature::Todos, decider, format!("tododone:{id}"), body);
        }
    }

    /// What to ask Jev about the board just composed: each feature that's on looks for what's
    /// changed since it last asked.
    pub(super) fn jev_look(&mut self, board: &Board) {
        if !self.cfg.decisions.any() {
            return;
        }
        if self.cfg.decisions.todos {
            self.jev_look_todos(board);
        }
        if self.cfg.decisions.sessions {
            self.look_insights(board);
        }
    }

    /// Asks Jev about sessions whose moment it hasn't read yet (insights.rs). At most a few new
    /// questions per board, so the first run after turning it on doesn't ask about every session
    /// at once. What a session shows may be an agent's tag or its own status line (`with_insights`),
    /// so whether Jev has answered is the store's to say, not the board's.
    fn look_insights(&mut self, board: &Board) {
        const PER_BOARD: usize = 20;
        let now = now_ms();
        let mut asks = Vec::new();
        for (lane, s) in board.lanes.iter().flat_map(|l| l.sessions.iter().map(move |s| (l, s))).filter(|(_, s)| s.visitor.is_none()) {
            self.insights.seen(&s.key, now);
            // Its project or boundary group can keep a session from every decision model, and
            // says which one may read it (decisions.rs).
            let Some(decider) = crate::decisions::of_session(&self.cfg, &board.lanes, lane, s) else { continue };
            let Some((kind, signature)) = crate::insights::ask(s, now) else { continue };
            let id = format!("insight:{signature}|{}", s.key);
            if asks.len() < PER_BOARD && self.insights.wants(&s.key, &signature, now) && !self.jev_pending.contains(&id) {
                let body = crate::insights::request(s, kind, decider.model(), None);
                // An approval is judged by the whole call, which its harness may take a slow read
                // to show (OpenCode's over HTTP): read on the worker's thread, never on this one.
                let fill: Option<Box<dyn FnOnce(&mut serde_json::Value) + Send>> = match kind {
                    crate::insights::Kind::Approval => {
                        let (s, account, model) = (s.clone(), self.cfg.accounts.iter().find(|a| a.name == s.account).cloned(), decider.model().to_string());
                        Some(Box::new(move |body: &mut serde_json::Value| {
                            if let Some(waiting) = Self::waiting_call(&s, account.as_ref()) {
                                *body = crate::insights::request(&s, crate::insights::Kind::Approval, &model, Some(&waiting));
                            }
                        }))
                    }
                    _ => None,
                };
                asks.push((id, decider, body, fill));
            }
        }
        for (id, decider, body, fill) in asks {
            self.ask_jev_filled(jev::Feature::Sessions, decider, id, body, fill);
        }
        self.insights.save(now);
    }

    /// Jev's read of a session, kept under the moment it was asked about.
    fn on_insight(&mut self, rest: String, answer: std::result::Result<serde_json::Value, String>) {
        let Some((signature, key)) = rest.split_once('|') else { return };
        let now = now_ms();
        match (answer, crate::insights::kind_of(signature)) {
            (Ok(v), Some(kind)) => {
                self.insights.answered(key.to_string(), signature.to_string(), crate::insights::read(kind, &v), now);
                self.insights.save(now);
                self.mark_dirty("Jev's insight");
            }
            _ => self.insights.failed(signature.to_string(), now),
        }
    }

    /// The whole call a session waits on you to allow, as its harness shows it, with its name
/// (`insights::waiting_words`), for Jev to judge its risk by: the arguments, not just the line the
/// excerpt has. A read that may be slow: run on Jev's thread, not the engine's. A command its agent
/// asks to run in your terminal comes first, whole, as `session-pending` gives it (actions.rs).
fn waiting_call(s: &Session, account: Option<&config::AccountCfg>) -> Option<String> {
    if let Some(asked) = crate::terminal::of_session(&s.key) {
        return crate::insights::waiting_words(&crate::terminal::pending(&asked, "your terminal"));
    }
    let pending = if crate::acp::hosts(&s.key) { crate::acp::pending(&s.key) } else { crate::harness::adapter(s.harness).pending(&crate::harness::Ctx { account: account? }, s) };
    crate::insights::waiting_words(&pending?)
}
}

#[cfg(test)]
mod tests {
    /// Jev judges the risk of a command an agent asks to run in your terminal from all of it, not
    /// the first line the session's excerpt has.
    #[test]
    fn jev_reads_a_command_for_your_terminal_whole() {
        use super::*;
        let key = "codex:codex:engine-terminal-risk";
        let asked = crate::terminal::Asked { id: "run-engine-risk".into(), ask: crate::terminal::Ask { session: key.into(), command: "echo tidying up\nrm -rf ~/projects".into(), cwd: "/tmp".into(), ..Default::default() }, at: 1 };
        let _held = crate::terminal::held_for_tests(asked);
        let words = Engine::waiting_call(&crate::fixtures::session(key), None).unwrap();
        assert!(words.starts_with("Run in your terminal") && words.ends_with("echo tidying up\nrm -rf ~/projects"), "{words}");
        crate::terminal::deny(key, "run-engine-risk", None);
    }
}
