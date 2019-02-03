use std::fs::File;
use std::path::Path;

use crate::config::Config;
use crate::error::*;
use crate::repo_info::RepoInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(flatten)]
    pub config: Config,

    #[serde(default)]
    pub repos: Vec<RepoEntry>,
}

impl WorkspaceConfig {
    pub const DEFAULT_PATH: &'static str = "config.yml";

    pub fn exists(dir: &Path) -> bool {
        Path::new(dir).join(Self::DEFAULT_PATH).exists()
    }

    pub fn from_dir(dir: &Path) -> Result<Self> {
        Self::from_path(&Path::new(dir).join(Self::DEFAULT_PATH))
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        Self::from_file(&file)
    }

    pub fn from_file(file: &File) -> Result<Self> {
        match serde_yaml::from_reader::<_, WorkspaceConfig>(file) {
            Ok(mut config) => {
                config.config.maybe_apply_base();
                Ok(config)
            }
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn add(&mut self, repo_info: RepoInfo, path: Option<String>) -> &RepoEntry {
        // TODO: dedup against existing repos
        let entry = RepoEntry {
            repo_info: repo_info,
            ignore: false,
            path,
        };
        self.repos.push(entry);
        self.repos.last().unwrap()
    }

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
    pub ignore: bool,

    #[serde(default)]
    pub path: Option<String>,
}

impl RepoEntry {
    pub fn path(&self) -> &Path {
        Path::new(self.path.as_ref().unwrap_or_else(|| &self.repo_info.name))
    }
}
