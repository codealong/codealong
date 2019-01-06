use chrono::prelude::*;
use chrono::DateTime;

use crate::repo::Repo;
use crate::user::User;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequest {
    pub id: u64,
    pub url: Option<String>,
    pub number: u64,
    pub base: Ref,
    pub head: Ref,
    pub html_url: Option<String>,
    pub state: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub user: User,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub merged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ref {
    pub sha: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub repo: Option<Repo>,
}
