use chrono::prelude::*;
use chrono::DateTime;
use std::borrow::Cow;
use std::collections::HashSet;

pub trait Event {
    fn id(&self) -> Cow<str>;
    fn timestamp(&self) -> &DateTime<Utc>;
    fn event_type(&self) -> &str;
    fn tags(&self) -> HashSet<String>;
}
