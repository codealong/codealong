#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeOpts {
    pub ignore_unknown_authors: bool,
}

impl Default for AnalyzeOpts {
    fn default() -> Self {
        AnalyzeOpts {
            ignore_unknown_authors: false,
        }
    }
}
