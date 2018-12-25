use crate::analyzed_diff::AnalyzedDiff;
use crate::event::Event;
use crate::identity::Identity;

use chrono::prelude::*;
use chrono::{DateTime, FixedOffset, TimeZone};
use git2::{Commit, Time};
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzedCommit {
    pub id: String,
    #[serde(flatten)]
    pub diff: AnalyzedDiff,
    pub summary: Option<String>,
    pub author: Identity,
    pub authored_at: DateTime<Utc>,
    pub author_id: Option<String>,
    pub committer: Identity,
    pub committed_at: DateTime<Utc>,
    pub committer_id: Option<String>,
    pub repo_name: Option<String>,
    pub github_url: Option<String>,
}

impl AnalyzedCommit {
    pub fn new(commit: &Commit) -> AnalyzedCommit {
        AnalyzedCommit {
            id: commit.id().to_string(),
            diff: AnalyzedDiff::empty(),
            summary: commit.summary().map(|s| s.to_string()),
            author: Identity::from(commit.author()),
            authored_at: convert_time(&commit.author().when()),
            author_id: None,
            committer: Identity::from(commit.committer()),
            committed_at: convert_time(&commit.committer().when()),
            committer_id: None,
            repo_name: None,
            github_url: None,
        }
    }

    pub fn merge_diff(&mut self, diff: &AnalyzedDiff) {
        self.diff = &self.diff + diff;
    }
}

impl Event for AnalyzedCommit {
    fn timestamp(&self) -> &DateTime<Utc> {
        &self.authored_at
    }

    fn event_type(&self) -> &str {
        "commit"
    }

    fn id(&self) -> Cow<str> {
        Cow::Borrowed(&self.id)
    }
}

fn convert_time(time: &Time) -> DateTime<Utc> {
    let tz = FixedOffset::east(time.offset_minutes() * 60);
    tz.timestamp(time.seconds(), 0).with_timezone(&Utc)
}
