use analyzed_pull_request::AnalyzedPullRequest;
use pull_request::PullRequest;

pub struct PullRequestAnalyzer {
    pr: PullRequest,
}

impl PullRequestAnalyzer {
    pub fn new(pr: PullRequest) -> PullRequestAnalyzer {
        PullRequestAnalyzer { pr }
    }

    pub fn analyze(self) -> AnalyzedPullRequest {
        // TODO analyze head..base
        AnalyzedPullRequest::new(self.pr)
    }
}
