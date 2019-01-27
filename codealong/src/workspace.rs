use std::path::{Path, PathBuf};

use crate::repo_info::RepoInfo;
use crate::workspace_config::WorkspaceConfig;

/// Represents a working directory for analyzing multiple repositories. This
/// directory contains all of the working copies of repositories that are under
/// analysis as well as a root configuration file.
pub struct Workspace {
    dir: PathBuf,
    config: WorkspaceConfig,
}

impl Workspace {
    pub fn repos_dir(&self) -> PathBuf {
        self.dir.join(".repos")
    }

    pub fn repo_dir(&self, repo_info: &RepoInfo) -> PathBuf {
        self.repos_dir().join(repo_info.name.to_owned())
    }
}
