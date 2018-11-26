use chrono::prelude::*;
use chrono::DateTime;
use std::borrow::Cow;

use codealong::Event;

use pull_request::PullRequest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzedPullRequest {
    timestamp: DateTime<Utc>,

    #[serde(flatten)]
    pr: PullRequest,
}

impl AnalyzedPullRequest {
    pub fn new(pr: PullRequest) -> AnalyzedPullRequest {
        AnalyzedPullRequest {
            timestamp: pr.merged_at.unwrap_or(pr.updated_at),
            pr,
        }
    }
}

impl Event for AnalyzedPullRequest {
    fn timestamp(&self) -> &DateTime<Utc> {
        return &self.timestamp;
    }

    fn id(&self) -> Cow<str> {
        self.pr.id.to_string().into()
    }

    fn event_type(&self) -> &str {
        "pull_request"
    }
}
