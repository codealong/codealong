use analyzed_diff::AnalyzedDiff;

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedCommit {
    pub diff: AnalyzedDiff,
}

impl AnalyzedCommit {
    pub fn new() -> AnalyzedCommit {
        AnalyzedCommit {
            diff: AnalyzedDiff::empty(),
        }
    }

    pub fn merge_diff(&mut self, diff: &AnalyzedDiff) {
        self.diff = &self.diff + diff;
    }
}
