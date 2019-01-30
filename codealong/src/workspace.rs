use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::*;
use crate::repo::Repo;
use crate::repo_info::RepoInfo;
use crate::workspace_config::{RepoEntry, WorkspaceConfig};

/// Represents a working directory for analyzing multiple repositories. This
/// directory contains all of the working copies of repositories that are under
/// analysis as well as a root configuration file.
pub struct Workspace {
    dir: PathBuf,
    config: WorkspaceConfig,
}

impl Workspace {
    /// Returns a workspace if the current directory has a config
    pub fn discover(dir: PathBuf) -> Option<Result<Workspace>> {
        if WorkspaceConfig::exists(&dir) {
            Some(Self::from_dir(dir))
        } else {
            None
        }
    }

    /// Return a discovered workspace or the default.
    pub fn current(dir: PathBuf) -> Result<Workspace> {
        Self::discover(dir).unwrap_or_else(|| Self::default())
    }

    /// The default workspace.
    pub fn default() -> Result<Workspace> {
        Self::from_dir(dirs::home_dir().unwrap().join(".codealong"))
    }

    /// Similar to new, but reads existing configs, creating a new one if none
    /// is present.
    pub fn from_dir(dir: PathBuf) -> Result<Workspace> {
        let config = match WorkspaceConfig::from_dir(&dir) {
            Ok(config) => config,
            Err(_) => WorkspaceConfig::default(),
        };
        Ok(Self::new(dir, config))
    }

    pub fn new(dir: PathBuf, config: WorkspaceConfig) -> Workspace {
        Workspace { dir, config }
    }

    pub fn repos_dir(&self) -> &Path {
        &self.dir
    }

    pub fn repo_dir(&self, repo_entry: &RepoEntry) -> PathBuf {
        self.repos_dir().join(&repo_entry.path())
    }

    pub fn repos(&self) -> Vec<Repo> {
        self.config
            .repos
            .iter()
            .map(|entry| self.repo(&entry))
            .collect()
    }

    pub fn repo(&self, entry: &RepoEntry) -> Repo {
        let path = self.repo_dir(&entry);
        Repo::new(self.config.config.clone(), path, entry.repo_info.clone())
    }

    pub fn add(&mut self, repo_info: RepoInfo, path: Option<String>) -> Result<()> {
        self.config.add(repo_info, path);
        Ok(())
    }

    pub fn add_config(&mut self, config: Config) {
        self.config.config.merge(config);
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover() -> Result<()> {
        assert!(!Workspace::discover(Path::new(".").to_owned()).is_some());
        assert!(
            Workspace::discover(Path::new("./fixtures/workspaces/serde-rs").to_owned()).is_some()
        );
        Ok(())
    }
}
