use crate::config::Config;
use crate::repo_info::RepoInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(flatten)]
    pub config: Config,

    #[serde(default)]
    pub repos: Vec<RepoEntry>,
}

impl WorkspaceConfig {
    pub fn merge(&mut self, other: WorkspaceConfig) {
        self.repos.extend(other.repos);
        self.config.merge(other.config);
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        WorkspaceConfig {
            config: Config::default(),
            repos: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoEntry {
    #[serde(flatten, default)]
    pub repo_info: RepoInfo,

    #[serde(default)]
    pub path: Option<String>,
}
