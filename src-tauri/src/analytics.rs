//! Your part in the work, as one report (with `[experimental] analytics` on): what came in to
//! your sessions and who from, what your prompts did with what the agents had said (turns.rs,
//! stance.rs), how sessions began (began.rs) and how the ones you were done with stood
//! (dismissed.rs). The Analytics window's Your part and Sessions show it, beside what the same days
//! cost by who began the work (`ledger::Report::hands`, `origins`), and `codealong analytics`
//! prints it.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::dismissed::Outcomes;
use crate::model::Board;
use crate::turns::Part;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    /// `[experimental] analytics` is on. Off, nothing is kept and the rest is empty.
    pub on: bool,
    pub part: Part,
    pub outcomes: Outcomes,
    /// When the ledger last caught up with every record; 0 before its first read ends, and
    /// whether that first read is still going.
    pub as_of: i64,
    pub filling: bool,
}

/// The report for the last `days` days, for one account or all, from the app's ledger. `board`
/// is where the sessions left hanging are looked for.
pub fn report(board: Option<&Board>, days: u32, account: Option<&str>) -> Result<Analytics> {
    let cfg = crate::config::current()?;
    if !cfg.experimental.analytics {
        return Ok(Analytics::default());
    }
    let mut r = crate::ledger::with_db(|db| report_in(db, &cfg, board, days, account, crate::util::now_ms()))?;
    (r.as_of, r.filling) = crate::ledger::caught_up();
    Ok(r)
}

pub fn report_in(db: &Connection, cfg: &Config, board: Option<&Board>, days: u32, account: Option<&str>, now: i64) -> Result<Analytics> {
    Ok(Analytics { on: true, part: crate::turns::part_in(db, cfg, days, account, now)?, outcomes: crate::dismissed::outcomes_in(db, board, days, account, now)?, as_of: 0, filling: false })
}

/// Says what a prompt of yours did, as an agent that read its conversation has it (`codealong
/// analytics tag`): its word stands over Jev's read. `by` is the session that said so, when the
/// command found one.
pub fn tag(db: &Connection, id: &str, stance: &str, by: Option<&str>) -> Result<()> {
    let row: i64 = id.trim().parse().map_err(|_| anyhow::anyhow!("“{id}” isn't a prompt's id: `codealong analytics prompts` lists them."))?;
    crate::stance::set(db, row, stance, None, &by.map(|k| format!("agent {k}")).unwrap_or_else(|| "agent".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_tag_stands_as_what_a_prompt_did() {
        let db = crate::ledger::in_memory();
        let sid = crate::ledger::session_row(&db, "claude:claude:a", "claude", "/work/app").unwrap();
        let now = crate::util::now_ms();
        db.execute("INSERT INTO turns (id, session, at, source, kind, text, before, counts_as) VALUES (7, ?1, ?2, 'you', 'prompt', 'No, wrap it instead.', 'I will rewrite it.', 'you')", rusqlite::params![sid, now - 1000]).unwrap();
        assert_eq!(crate::stance::unread(&db, 10).unwrap().len(), 1);
        assert!(tag(&db, "7", "approves", None).unwrap_err().to_string().contains("isn't something a prompt does"));
        assert!(tag(&db, "nine", "steers", None).is_err() && tag(&db, "8", "steers", None).is_err());
        tag(&db, "7", "redirects", Some("claude:claude:o")).unwrap();
        let (stance, by): (String, String) = db.query_row("SELECT stance, read_by FROM turns WHERE id = 7", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((stance.as_str(), by.as_str()), ("redirects", "agent claude:claude:o"));
        assert!(crate::stance::unread(&db, 10).unwrap().is_empty());
        let r = report_in(&db, &Config::default(), None, 7, None, now).unwrap();
        assert!(r.on);
        assert_eq!(r.part.stances, vec![crate::turns::Count { key: "redirects".into(), n: 1 }]);
        assert_eq!(r.part.prompts.len(), 1);
    }
}
