extern crate hostname;
use codealong::AnalyzedCommit;

use chrono::prelude::*;
use chrono::DateTime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "@timestamp")]
    timestamp: DateTime<Utc>,

    #[serde(rename = "@version")]
    version: u64,

    host: Option<String>,

    #[serde(rename = "type")]
    event_type: String,

    #[serde(flatten)]
    commit: AnalyzedCommit,
}

impl Event {
    pub fn new(analyzed_commit: AnalyzedCommit) -> Event {
        Event {
            event_type: "commit".to_owned(),
            version: 1,
            host: hostname::get_hostname(),
            timestamp: analyzed_commit.authored_at.clone(),
            commit: analyzed_commit,
        }
    }

    pub fn id(&self) -> &str {
        &self.commit.id
    }

    pub fn timestamp(&self) -> &DateTime<Utc> {
        &self.timestamp
    }
}
