use crate::config::Config;
use crate::repo_info::RepoInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(flatten)]
    pub config: Config,

    #[serde(default)]
    pub repos: Vec<RepoInfo>,
}
