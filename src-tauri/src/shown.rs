//! What the windows show: this Mac's board, and beside it your other Macs' worktrees as peers.rs
//! hands them to the windows (every id with `@<short>:` in front, each worktree tagged with its
//! device and put into this Mac's projects). The board is what the engine acts on and what it
//! sends your other Macs, so their worktrees never go into it: they're kept here (`Elsewhere`),
//! and whatever the engine cuts for a window that doesn't get the board (the bar's feed, a card
//! on the desktop, the lines between cards, a session for the peek or a pill's menu) is cut from
//! both at once (`Shown`). The windows that get the board put the two together the same way from
//! the `board` and `devices` events (src/useDevices.ts).

use crate::model::{Board, Lane, Session};

/// Your other Macs' worktrees, as peers.rs last handed them to the windows: those of a Mac that's
/// away too, as they were when it was last seen. Their tags say which are current
/// (`DeviceTag::online`, peers.rs `caught_up`).
#[derive(Default)]
pub struct Elsewhere {
    lanes: Vec<Lane>,
}

impl Elsewhere {
    /// Your devices' worktrees as the windows got them last (peers.rs `DevicesView::lanes`).
    pub fn on_devices(&mut self, lanes: &[Lane]) {
        self.lanes = lanes.to_vec();
    }

    /// What the windows show with `board`.
    pub fn beside<'a>(&'a self, board: &'a Board) -> Shown<'a> {
        Shown { board, elsewhere: &self.lanes }
    }
}

/// A board and your other Macs' worktrees beside it.
#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub board: &'a Board,
    pub elsewhere: &'a [Lane],
}

impl<'a> Shown<'a> {
    /// The board alone, for what has no other Macs to show (a snapshot).
    pub fn here(board: &'a Board) -> Shown<'a> {
        Shown { board, elsewhere: &[] }
    }

    /// This Mac's worktrees, then the other Macs'.
    pub fn lanes(&self) -> impl Iterator<Item = &'a Lane> {
        self.board.lanes.iter().chain(self.elsewhere)
    }

    /// The other Macs' worktrees that are current: their Mac is connected and has said since.
    pub fn connected(&self) -> impl Iterator<Item = &'a Lane> {
        self.elsewhere.iter().filter(|l| l.device.as_ref().is_some_and(|d| d.online))
    }

    /// A session with the worktree it's listed under as its own, else one it visits
    /// (`board::home`), on whichever Mac its key names.
    pub fn session(&self, key: &str) -> Option<(&'a Lane, &'a Session)> {
        crate::board::home(self.lanes_of(key), key)
    }

    /// A worktree by its id, on whichever Mac its id names.
    pub fn lane(&self, id: &str) -> Option<&'a Lane> {
        self.lanes_of(id).iter().find(|l| l.id == id)
    }

    /// The worktrees a key or an id can be among: another Mac's start `@<short>:`.
    fn lanes_of(&self, key: &str) -> &'a [Lane] {
        if crate::peerfeed::split_remote(key).is_some() {
            self.elsewhere
        } else {
            &self.board.lanes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DeviceTag;

    fn there(id: &str, online: bool, keys: &[&str]) -> Lane {
        let tag = DeviceTag { name: "Mac mini".into(), short: "abc".into(), color: "teal".into(), online, last_seen: None };
        Lane { device: Some(tag), ..crate::fixtures::lane(id, keys.iter().map(|k| crate::fixtures::session(k)).collect()) }
    }

    #[test]
    fn finds_a_session_or_a_worktree_on_the_mac_its_key_names() {
        let board = Board { lanes: vec![crate::fixtures::lane("/a", vec![crate::fixtures::session("s1")])], ..Board::default() };
        let mut elsewhere = Elsewhere::default();
        elsewhere.on_devices(&[there("@abc:/b", true, &["@abc:s2"]), there("@abc:/c", false, &["@abc:s3"])]);
        let shown = elsewhere.beside(&board);
        assert_eq!(shown.session("s1").map(|(l, _)| l.id.as_str()), Some("/a"));
        assert_eq!(shown.session("@abc:s2").map(|(l, _)| l.id.as_str()), Some("@abc:/b"));
        // Away, it's still shown, as it was when last seen.
        assert_eq!(shown.session("@abc:s3").map(|(l, _)| l.id.as_str()), Some("@abc:/c"));
        assert_eq!(shown.lane("@abc:/c").map(|l| l.sessions.len()), Some(1));
        assert!(shown.session("@abc:s1").is_none() && shown.lane("/b").is_none());
        assert_eq!(shown.lanes().count(), 3);
        assert_eq!(shown.connected().map(|l| l.id.as_str()).collect::<Vec<_>>(), vec!["@abc:/b"]);
        assert_eq!(Shown::here(&board).lanes().count(), 1);
    }
}
