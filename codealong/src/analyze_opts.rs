use chrono::prelude::*;
use chrono::DateTime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeOpts {
    pub ignore_unknown_authors: bool,
    pub since: Option<DateTime<Utc>>,
}

impl Default for AnalyzeOpts {
    fn default() -> Self {
        AnalyzeOpts {
            ignore_unknown_authors: false,
            since: None,
        }
    }
}
