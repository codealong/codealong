use git2::{Commit, DiffLine, Repository};

use crate::error::Error;
use crate::fast_blame::FastBlame;
use crate::line_analyzer::LineAnalyzer;
use crate::work_stats::WorkStats;

pub struct HunkAnalyzer<'a> {
    repo: &'a Repository,
    commit: &'a Commit<'a>,
    blame: Option<FastBlame>,
    result: WorkStats,
    weight: f64,
}

impl<'a> HunkAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        commit: &'a Commit<'a>,
        blame: Option<FastBlame>,
        weight: f64,
    ) -> HunkAnalyzer<'a> {
        HunkAnalyzer {
            repo,
            commit,
            blame,
            weight,
            result: WorkStats::empty(),
        }
    }

    pub fn analyze_line(&mut self, diff_line: &DiffLine) -> Result<(), Error> {
        let analyzer = LineAnalyzer::new(self.repo, self.commit, diff_line, self.blame.as_ref());
        let result = analyzer.analyze()?;
        self.result += result;
        Ok(())
    }

    pub fn finish(self) -> (Option<FastBlame>, WorkStats) {
        let mut result = self.result;
        result.impact = calculate_impact(&result, self.weight);
        (self.blame, result)
    }
}

/// Current line impact calculation is a simple function that tries to
/// incoporate the following ideas:
///
/// 1. Large hunks of code have less cognitive overhead on a per-line basis
///    than small or single-line changes. To incorporate this, the impact
///    of a hunk grows sub-linearly with its size.
///
/// 2. Different types of work have less cognitive overhead. Dealing with
///    refactorings of legacy code requires more context than greenfield
///    code. Similarly, helping others by changing their recently added code
///    also has more overhead than greenfield, but less than legacy. This
///    is incorporated into the calculation by a constant multiplier based
///    on work type. Churn does not have positive impact and is removed from
///    the calculation.
///
/// 3. Different languages, file-types, and repositories carry different
///    cognitive burdens. This is where Configuration-based weights of work
///    stats comes into play. The "weight" field is used as a multiplier.
fn calculate_impact(work_stats: &WorkStats, weight: f64) -> u64 {
    let line_value =
        work_stats.legacy_refactor * 4 + work_stats.help_others * 2 + work_stats.new_work;
    let scaled_line_value = (line_value as f64).powf(0.5);
    (scaled_line_value * weight).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impact() {
        assert_eq!(calculate_impact(&WorkStats {
            new_work: 100,
            ..Default::default()
        }, 1.0), 10);

        assert_eq!(calculate_impact(&WorkStats {
            legacy_refactor: 100,
            ..Default::default()
        }, 1.0), 20);

        assert_eq!(calculate_impact(&WorkStats {
            churn: 100,
            ..Default::default()
        }, 1.0), 0);
    }
}
