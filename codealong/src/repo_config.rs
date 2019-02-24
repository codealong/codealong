use std::fs::File;
use std::path::Path;

use git2::Repository;

use crate::config::Config;
use crate::error::*;
use crate::repo_info::RepoInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoConfig {
    #[serde(flatten)]
    pub config: Config,

    #[serde(default, flatten)]
    pub repo: RepoInfo,
}

impl RepoConfig {
    pub const DEFAULT_PATH: &'static str = ".codealong.yml";

    pub fn exists(dir: &Path) -> bool {
        Path::new(dir).join(Self::DEFAULT_PATH).exists()
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        Self::from_file(&file)
    }

    pub fn from_file(file: &File) -> Result<Self> {
        Ok(serde_yaml::from_reader::<_, RepoConfig>(file)?)
    }

    /// Attempts to read the config from the conventional location within the
    /// directory at `.codealong/config.yml`. If no config is found, fallback
    /// to the base config.
    ///
    /// If the config has no `name`, then default to the name of the directory.
    pub fn from_dir(path: &Path) -> Result<Self> {
        Self::from_repository(&Repository::discover(path)?)
    }

    pub fn from_repository(repo: &Repository) -> Result<Self> {
        // TODO once we go to bare repos we need to
        // read the object directly from git
        let path = repo.path();
        let file_path = path.join(".codealong").join(Self::DEFAULT_PATH);
        let mut config = if file_path.exists() {
            let mut config = Self::from_path(&file_path)?;
            config
        } else {
            Self::default()
        };
        // TODO: merge this
        config.repo = RepoInfo::from_repository(&repo)?;
        Ok(config)
    }

    pub fn merge(&mut self, other: RepoConfig) {
        if let None = self.repo.github_name {
            self.repo.github_name = other.repo.github_name.clone();
        }
        self.config.merge(other.config);
    }
}

impl Default for RepoConfig {
    fn default() -> Self {
        RepoConfig {
            config: Config::default(),
            repo: RepoInfo::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_dir_without_config() {
        let config = RepoConfig::from_dir(Path::new("fixtures/repos/simple")).unwrap();
        assert_eq!(config.repo.name, "simple");
    }

    #[test]
    fn test_from_dir_with_config() {
        let config = RepoConfig::from_dir(Path::new("fixtures/repos/bare_config")).unwrap();
        assert_eq!(config.repo.name, "bare_config");
    }

    #[test]
    fn test_from_repository() {
        let config =
            RepoConfig::from_repository(&Repository::open("fixtures/repos/bare_config").unwrap())
                .unwrap();
        assert_eq!(config.repo.github_name, None);
        let config = RepoConfig::from_repository(&Repository::open_from_env().unwrap()).unwrap();
        assert_eq!(
            config.repo.github_name,
            Some("ghempton/codealong".to_owned())
        );
    }
}
