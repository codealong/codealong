use chrono::prelude::*;
use chrono::DateTime;
use std::borrow::Cow;

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
            pr,
            diff,
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
}
