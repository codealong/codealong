extern crate hostname;
use codealong;

use chrono::prelude::*;
use chrono::DateTime;

use std::borrow::Cow;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event<T: codealong::Event> {
    #[serde(rename = "@timestamp")]
    timestamp: DateTime<Utc>,

    #[serde(rename = "@version")]
    version: u64,

    host: Option<String>,

    #[serde(rename = "type")]
    event_type: String,

    #[serde(flatten)]
    inner: T,

    tags: HashSet<String>,
}

impl<T> Event<T>
where
    T: codealong::Event,
{
    pub fn new(inner: T) -> Event<T> {
        Event {
            event_type: inner.event_type().to_string(),
            version: 1,
            host: hostname::get_hostname(),
            timestamp: inner.timestamp().clone(),
            tags: inner.tags(),
            inner: inner,
        }
    }

    pub fn id(&self) -> Cow<str> {
        self.inner.id()
    }

    pub fn timestamp(&self) -> &DateTime<Utc> {
        &self.timestamp
    }
}
