use chrono::prelude::*;
use chrono::DateTime;
use std::borrow::Cow;
use std::collections::HashSet;
use std::iter::FromIterator;

use codealong::{AnalyzedDiff, Event, Person};

use crate::pull_request::PullRequest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzedPullRequest {
    timestamp: DateTime<Utc>,

    normalized_author: Person,

    #[serde(flatten)]
    pr: PullRequest,

    #[serde(flatten)]
    pub diff: Option<AnalyzedDiff>,

    pub time_to_resolve: Option<i64>,
}

impl AnalyzedPullRequest {
    pub fn new(
        pr: PullRequest,
        diff: Option<AnalyzedDiff>,
        normalized_author: Person,
    ) -> AnalyzedPullRequest {
        AnalyzedPullRequest {
            timestamp: pr.merged_at.unwrap_or(pr.updated_at),
            normalized_author,
            diff,
            time_to_resolve: pr
                .merged_at
                .as_ref()
                .map(|ma| (ma.clone() - pr.created_at.clone()).num_seconds()),
            pr,
        }
    }
}

impl Event for AnalyzedPullRequest {
    fn timestamp(&self) -> &DateTime<Utc> {
        &self.timestamp
    }

    fn id(&self) -> Cow<str> {
        self.pr.id.to_string().into()
    }

    fn event_type(&self) -> &str {
        "pull_request"
    }

    fn tags(&self) -> HashSet<String> {
        if let Some(ref diff) = self.diff {
            HashSet::from_iter(diff.tag_stats.keys().map(|s| s.to_owned()))
        } else {
            HashSet::new()
        }
    }
}
