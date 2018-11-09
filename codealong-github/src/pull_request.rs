use chrono::prelude::*;
use chrono::DateTime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequest {
    pub id: u64,
    pub url: String,
    pub number: u64,
    pub base: Ref,
    pub head: Ref,
    pub html_url: String,
    pub state: String,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub merged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ref {
    pub sha: String,
}

impl PullRequest {}
