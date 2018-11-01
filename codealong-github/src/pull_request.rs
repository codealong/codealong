#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequest {
    id: u64,
    url: String,
    number: u64,
}

impl PullRequest {}
