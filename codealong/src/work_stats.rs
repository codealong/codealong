use std::ops::{Add, AddAssign};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkStats {
    pub new_work: u64,
    pub legacy_refactor: u64,
    pub churn: u64,
    pub help_others: u64,
    pub other: u64,
}

impl WorkStats {
    pub fn empty() -> WorkStats {
        WorkStats {
            new_work: 0,
            legacy_refactor: 0,
            churn: 0,
            help_others: 0,
            other: 0,
        }
    }

    pub fn new_work() -> WorkStats {
        WorkStats {
            new_work: 1,
            legacy_refactor: 0,
            churn: 0,
            help_others: 0,
            other: 0,
        }
    }

    pub fn legacy_refactor() -> WorkStats {
        WorkStats {
            new_work: 0,
            legacy_refactor: 1,
            churn: 0,
            help_others: 0,
            other: 0,
        }
    }

    pub fn churn() -> WorkStats {
        WorkStats {
            new_work: 0,
            legacy_refactor: 0,
            churn: 1,
            help_others: 0,
            other: 0,
        }
    }

    pub fn help_others() -> WorkStats {
        WorkStats {
            new_work: 0,
            legacy_refactor: 0,
            churn: 0,
            help_others: 1,
            other: 0,
        }
    }

    pub fn other() -> WorkStats {
        WorkStats {
            new_work: 0,
            legacy_refactor: 0,
            churn: 0,
            help_others: 0,
            other: 1,
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
    }
}
