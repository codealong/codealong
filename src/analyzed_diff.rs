use std::collections::HashMap;
use std::ops::{Add, AddAssign};
use work_stats::WorkStats;

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedDiff {
    pub stats: WorkStats,
    pub tag_stats: HashMap<String, WorkStats>,
}

impl AnalyzedDiff {
    pub fn empty() -> AnalyzedDiff {
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
    fn it_adds() {
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
}
