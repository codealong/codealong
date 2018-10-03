use analyzed_diff::AnalyzedDiff;

use chrono::prelude::*;
use chrono::{DateTime, FixedOffset, TimeZone};
use git2::{Commit, Time};

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedCommit {
    pub id: String,
    pub diff: AnalyzedDiff,
    pub summary: Option<String>,
    pub author_email: Option<String>,
    pub author_name: Option<String>,
    pub authored_at: DateTime<Utc>,
    pub committer_email: Option<String>,
    pub committer_name: Option<String>,
    pub committed_at: DateTime<Utc>,
}

impl AnalyzedCommit {
    pub fn new(commit: &Commit) -> AnalyzedCommit {
        AnalyzedCommit {
            id: commit.id().to_string(),
            diff: AnalyzedDiff::empty(),
            summary: commit.summary().map(|s| s.to_string()),
            author_email: commit.author().email().map(|s| s.to_string()),
            author_name: commit.author().name().map(|s| s.to_string()),
            authored_at: convert_time(&commit.author().when()),
            committer_email: commit.committer().email().map(|s| s.to_string()),
            committer_name: commit.committer().name().map(|s| s.to_string()),
            committed_at: convert_time(&commit.committer().when()),
        }
    }

    pub fn merge_diff(&mut self, diff: &AnalyzedDiff) {
        self.diff = &self.diff + diff;
    }
}

fn convert_time(time: &Time) -> DateTime<Utc> {
    let tz = FixedOffset::east(time.offset_minutes() * 60);
    tz.timestamp(time.seconds(), 0).with_timezone(&Utc)
}
