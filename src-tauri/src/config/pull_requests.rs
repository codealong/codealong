//! `[pull_requests]` in config.toml: PR nudging, triage, bot allowlisting, re-review requests,
//! and review queue priority.

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

/// Your PRs (nudges.rs). A PR's own switch, on its card, its menu or `codealong pr nudge`, wins
/// over this either way.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestsCfg {
    /// Every open PR of yours tells its primary session (board.rs `pr_primary`) what happens on
    /// it. Off, a PR tells nobody until turned on for it.
    #[serde(default)]
    pub nudge_by_default: bool,
    /// What a PR whose nudge is on tells its session about.
    #[serde(default)]
    pub nudge_about: NudgeAbout,
    /// Run a read-only side-fork triage helper (nudges/triage.rs) before notifying or waking a
    /// PR's primary session on PR nudges and automatic own-PR review handbacks.
    #[serde(default = "yes")]
    pub triage_nudges: bool,
    /// Bot logins allowed to trigger PR nudges (empty ignores all bots; `"*"` allows every bot).
    #[serde(default)]
    pub nudge_bots: Vec<String>,
    /// On a PR of yours whose reviewer asked for changes, ask them to review it again once
    /// commits came after their last word, nobody said so on it, and it's been quiet a while
    /// (rerequest.rs): GitHub tells a reviewer of a request, not of a push. It acts as you, so
    /// it's off until you turn it on.
    #[serde(default)]
    pub rerequest_after_fixes: bool,
    /// A PR waiting on your review goes ahead of everything else in what waits on you (src/queue.ts
    /// `byPlace`), in the notch, the waiting window and the quick panel's list. Off, reviews come
    /// where `[queue] order` puts them. A repo's team can turn it on for everyone (teamfile.rs),
    /// and your table for a repo wins over both.
    #[serde(default)]
    pub reviews_first: bool,
}

impl Default for PullRequestsCfg {
    fn default() -> Self {
        PullRequestsCfg { nudge_by_default: false, nudge_about: NudgeAbout::Everything, triage_nudges: true, nudge_bots: Vec::new(), rerequest_after_fixes: false, reviews_first: false }
    }
}

/// What a nudge tells a PR's session about (nudges.rs `NudgeAbout::keep`). `everything`: reviews,
/// comments, an approval, a merge or close, failing checks, and a branch that came to conflict
/// with its base. `feedback`: only what asks for work: comments, and reviews that ask for changes
/// or say something, never an approval, a merge or a close. `feedback-and-checks`: that, failing
/// checks and conflicts. `feedback-and-merges` and `feedback-checks-and-merges`: one of those, and
/// a merge, so the session can wrap its work up; still never an approval or a close.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NudgeAbout {
    #[default]
    Everything,
    Feedback,
    FeedbackAndChecks,
    FeedbackAndMerges,
    FeedbackChecksAndMerges,
}

impl NudgeAbout {
    pub fn as_str(self) -> &'static str {
        match self {
            NudgeAbout::Everything => "everything",
            NudgeAbout::Feedback => "feedback",
            NudgeAbout::FeedbackAndChecks => "feedback-and-checks",
            NudgeAbout::FeedbackAndMerges => "feedback-and-merges",
            NudgeAbout::FeedbackChecksAndMerges => "feedback-checks-and-merges",
        }
    }

    /// It tells of failing checks and conflicts.
    pub fn checks(self) -> bool {
        matches!(self, NudgeAbout::Everything | NudgeAbout::FeedbackAndChecks | NudgeAbout::FeedbackChecksAndMerges)
    }

    /// It tells of a merge.
    pub fn merges(self) -> bool {
        matches!(self, NudgeAbout::Everything | NudgeAbout::FeedbackAndMerges | NudgeAbout::FeedbackChecksAndMerges)
    }
}

/// `[pull_requests]` for one repo, key by key.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PullRequestsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudge_by_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudge_about: Option<NudgeAbout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triage_nudges: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudge_bots: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerequest_after_fixes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviews_first: Option<bool>,
}

impl PullRequestsPatch {
    pub fn is_empty(&self) -> bool {
        *self == PullRequestsPatch::default()
    }
}
