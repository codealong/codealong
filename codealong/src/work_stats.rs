use std::ops::{Add, AddAssign};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkStats {
    pub new_work: u64,
    pub legacy_refactor: u64,
    pub churn: u64,
    pub help_others: u64,
    pub other: u64,
    pub impact: u64,
}

impl WorkStats {
    pub fn empty() -> WorkStats {
        WorkStats::default()
    }

    pub fn new_work() -> WorkStats {
        WorkStats {
            new_work: 1,
            ..Default::default()
        }
    }

    pub fn legacy_refactor() -> WorkStats {
        WorkStats {
            legacy_refactor: 1,
            ..Default::default()
        }
    }

    pub fn churn() -> WorkStats {
        WorkStats {
            churn: 1,
            ..Default::default()
        }
    }

    pub fn help_others() -> WorkStats {
        WorkStats {
            help_others: 1,
            ..Default::default()
        }
    }

    pub fn other() -> WorkStats {
        WorkStats {
            other: 1,
            ..Default::default()
        }
    }
}

impl Default for WorkStats {
    fn default() -> WorkStats {
        WorkStats {
            new_work: 0,
            legacy_refactor: 0,
            churn: 0,
            help_others: 0,
            other: 0,
            impact: 0,
        }
    }
}

impl Add for WorkStats {
    type Output = WorkStats;

    fn add(self, other: WorkStats) -> Self::Output {
        self + &other
    }
}

impl<'b> Add<&'b WorkStats> for WorkStats {
    type Output = WorkStats;

    fn add(self, other: &'b WorkStats) -> Self::Output {
        &self + other
    }
}

impl<'a> Add<WorkStats> for &'a WorkStats {
    type Output = WorkStats;

    fn add(self, other: WorkStats) -> Self::Output {
        self + &other
    }
}

impl<'a, 'b> Add<&'b WorkStats> for &'a WorkStats {
    type Output = WorkStats;

    fn add(self, other: &'b WorkStats) -> Self::Output {
        WorkStats {
            new_work: self.new_work + other.new_work,
            legacy_refactor: self.legacy_refactor + other.legacy_refactor,
            churn: self.churn + other.churn,
            help_others: self.help_others + other.help_others,
            other: self.other + other.other,
            impact: self.impact + other.impact,
        }
    }
}

impl AddAssign for WorkStats {
    fn add_assign(&mut self, other: WorkStats) {
        *self += &other;
    }
}

impl<'a> AddAssign<&'a WorkStats> for WorkStats {
    fn add_assign(&mut self, other: &'a WorkStats) {
        self.new_work += other.new_work;
        self.legacy_refactor += other.legacy_refactor;
        self.churn += other.churn;
        self.help_others += other.help_others;
        self.other += other.other;
        self.impact += other.impact;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_work() {
        let stats = WorkStats::new_work();
        assert_eq!(stats.new_work, 1);
        assert_eq!(stats.legacy_refactor, 0);
        assert_eq!(stats.churn, 0);
        assert_eq!(stats.help_others, 0);
        assert_eq!(stats.other, 0);
        assert_eq!(stats.impact, 0);
    }
}
