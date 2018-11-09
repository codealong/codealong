use chrono::prelude::*;
use chrono::DateTime;
use std::borrow::Cow;

pub trait Event {
    fn id(&self) -> Cow<str>;
    fn timestamp(&self) -> &DateTime<Utc>;
    fn event_type(&self) -> &str;
}
