use std::fs::File;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::*;
use crate::repo_info::RepoInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(skip_serializing, default)]
    pub path: Option<PathBuf>,

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
        let mut res = Self::from_file(&file)?;
        res.path.replace(path.to_owned());
        Ok(res)
    }

    pub fn from_file(file: &File) -> Result<Self> {
        Ok(serde_yaml::from_reader::<_, WorkspaceConfig>(file)?)
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(self.path.as_ref().ok_or("No path specified")?)?;
        Ok(serde_yaml::to_writer(file, self)?)
    }

    /// Adds the repo to this configuration's list of repos. If a repo with the
    /// same name already exists, it is overwritten.
    pub fn add(&mut self, repo_info: RepoInfo, path: Option<String>) {
        let entry = RepoEntry {
            repo_info: repo_info,
            ignore: false,
            path,
        };

        match self
            .repos
            .iter_mut()
            .find(|ref entry| entry.repo_info.name == entry.repo_info.name)
        {
            Some(e) => {
                *e = entry;
            }
            None => {
                self.repos.push(entry);
            }
        }
    }

    pub fn get_entry(&self, name: &str) -> Option<&RepoEntry> {
        self.repos
            .iter()
            .find(|ref entry| entry.repo_info.name == name)
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
            path: None,
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
