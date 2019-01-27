#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoInfo {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub fork: bool,

    #[serde(default)]
    pub github_name: Option<String>,

    // intentionally left as string since Url does not support ssh+git style urls
    #[serde(default)]
    pub clone_url: String,

    #[serde(default)]
    pub refs: Vec<String>,
}

impl RepoInfo {
    pub fn partial(&self) -> PartialRepoInfo {
        PartialRepoInfo {
            name: self.name.clone(),
            fork: self.fork,
        }
    }
}

impl Default for RepoInfo {
    fn default() -> Self {
        RepoInfo {
            name: "".to_owned(),
            fork: false,
            github_name: None,
            clone_url: "".to_owned(),
            refs: vec!["origin/master".to_owned()],
        }
    }
}

/// Subset of RepoInfo that is included with each event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartialRepoInfo {
    name: String,
    fork: bool,
}
