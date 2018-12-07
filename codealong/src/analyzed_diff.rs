use std::collections::HashMap;
use std::ops::{Add, AddAssign};
use crate::work_stats::WorkStats;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzedDiff {
    pub stats: WorkStats,
    pub tag_stats: HashMap<String, WorkStats>,
}

impl AnalyzedDiff {
    pub fn empty() -> AnalyzedDiff {
        Default::default()
    }

    /// Adds the stats to all tags
    pub fn add_stats(&mut self, stats: WorkStats, tags: &Vec<String>) {
        self.stats += stats;
        for tag in tags {
            if self.tag_stats.contains_key(tag) {
                let v = self.tag_stats.get_mut(tag).unwrap();
                *v += stats;
            } else {
                self.tag_stats.insert(tag.to_string(), stats);
            }
        }
    }
}

impl Default for AnalyzedDiff {
    fn default() -> Self {
        AnalyzedDiff {
            stats: WorkStats::empty(),
            tag_stats: HashMap::new(),
        }
    }
}

impl Add for AnalyzedDiff {
    type Output = AnalyzedDiff;

    fn add(self, other: AnalyzedDiff) -> AnalyzedDiff {
        self + &other
    }
}

impl<'b> Add<&'b AnalyzedDiff> for AnalyzedDiff {
    type Output = AnalyzedDiff;

    fn add(self, other: &'b AnalyzedDiff) -> AnalyzedDiff {
        &self + other
    }
}

impl<'a> Add<AnalyzedDiff> for &'a AnalyzedDiff {
    type Output = AnalyzedDiff;

    fn add(self, other: AnalyzedDiff) -> Self::Output {
        self + &other
    }
}

impl<'a, 'b> Add<&'b AnalyzedDiff> for &'a AnalyzedDiff {
    type Output = AnalyzedDiff;

    fn add(self, other: &'b AnalyzedDiff) -> Self::Output {
        AnalyzedDiff {
            stats: self.stats + other.stats,
            tag_stats: merge_tag_stats(&self.tag_stats, &other.tag_stats),
        }
    }
}

impl AddAssign for AnalyzedDiff {
    fn add_assign(&mut self, other: AnalyzedDiff) {
        *self += &other;
    }
}

impl<'a> AddAssign<&'a AnalyzedDiff> for AnalyzedDiff {
    fn add_assign(&mut self, other: &'a AnalyzedDiff) {
        self.stats += other.stats;
        self.tag_stats = merge_tag_stats(&self.tag_stats, &other.tag_stats);
    }
}

fn merge_tag_stats(
    a: &HashMap<String, WorkStats>,
    b: &HashMap<String, WorkStats>,
) -> HashMap<String, WorkStats> {
    let mut res: HashMap<String, WorkStats> = HashMap::new();
    for (tag, count) in a.iter().chain(b.iter()) {
        let res_count = res.entry(tag.to_string()).or_insert(WorkStats::empty());
        *res_count += *count;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addition() {
        let mut tag_stats = HashMap::new();
        tag_stats.insert("migration".to_string(), WorkStats::new_work());
        let diff = AnalyzedDiff {
            stats: WorkStats::new_work(),
            tag_stats,
        };

        let mut tag_stats2 = HashMap::new();
        tag_stats2.insert("migration".to_string(), WorkStats::new_work());
        let diff2 = AnalyzedDiff {
            stats: WorkStats::new_work(),
            tag_stats: tag_stats2,
        };

        let result = diff + diff2;
        assert_eq!(result.stats.new_work, 2);
        assert_eq!(result.tag_stats.get("migration").unwrap().new_work, 2);
    }

    #[test]
    fn test_add_stats() {
        let mut diff = AnalyzedDiff::empty();
        diff.add_stats(
            WorkStats::legacy_refactor(),
            &vec!["ruby".to_string(), "rspec".to_string()],
        );
        assert_eq!(diff.tag_stats.len(), 2);
        assert_eq!(diff.tag_stats.get("ruby").unwrap().legacy_refactor, 1);
        assert_eq!(diff.tag_stats.get("rspec").unwrap().legacy_refactor, 1);
    }
}
