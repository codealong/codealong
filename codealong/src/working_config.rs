use glob::Pattern;
use std::collections::HashSet;

use crate::config::{Config, ContributorConfig, GlobConfig};
use crate::contributor::Contributor;
use crate::identity::Identity;

pub struct WorkingConfig {
    config: Config,
}

impl WorkingConfig {
    pub fn new(mut config: Config) -> WorkingConfig {
        if config.merge_defaults {
            config.merge(Config::base());
        }
        WorkingConfig { config }
    }

    pub fn churn_cutoff(&self) -> u64 {
        self.config.churn_cutoff
    }

    pub fn default() -> WorkingConfig {
        Self::new(Config::default())
    }

    pub fn config_for_file(&self, path: &str) -> Option<FileConfig> {
        let config = &self.config;
        let glob_configs: Vec<&GlobConfig> = config
            .files
            .iter()
            .filter_map(|(s, config)| {
                if let Ok(pattern) = Pattern::new(&s) {
                    if pattern.matches(path) {
                        Some(config)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        if glob_configs.is_empty() {
            None
        } else {
            Some(FileConfig::new(glob_configs))
        }
    }

    pub fn config_for_identity(&self, identity: &Identity) -> Option<&ContributorConfig> {
        let config = &self.config;
        for contributor_config in &config.contributors {
            for alias in &contributor_config.contributor.identities {
                if identity == alias {
                    return Some(contributor_config);
                }
            }
        }
        None
    }

    pub fn config_for_github_login(&self, github_login: &str) -> Option<&ContributorConfig> {
        let config = &self.config;
        for contributor_config in &config.contributors {
            for login in &contributor_config.contributor.github_logins {
                if login == github_login {
                    return Some(&contributor_config);
                }
            }
        }
        None
    }

    pub fn contributor_for_identity(&self, identity: &Identity) -> Contributor {
        if let Some(contributor_config) = self.config_for_identity(identity) {
            contributor_config.contributor.clone()
        } else {
            Contributor::from_identity(identity)
        }
    }

    pub fn contributor_for_github_login(&self, github_login: &str) -> Contributor {
        if let Some(contributor_config) = self.config_for_github_login(github_login) {
            contributor_config.contributor.clone()
        } else {
            Contributor::from_github_login(github_login)
        }
    }

    pub fn is_known(&self, identity: &Identity) -> bool {
        self.config_for_identity(identity).is_some()
    }

    pub fn is_github_login_known(&self, github_login: &str) -> bool {
        self.config_for_github_login(github_login).is_some()
    }
}

/// Represents multiple underlying glob-level configurations. A file can have
/// mulitiple configurations if it matches multiple globs.
pub struct FileConfig<'a> {
    configs: Vec<&'a GlobConfig>,
}

impl<'a> FileConfig<'a> {
    pub fn new(configs: Vec<&'a GlobConfig>) -> FileConfig<'a> {
        FileConfig { configs: configs }
    }

    pub fn tags(&self) -> HashSet<&str> {
        let mut res = HashSet::new();
        for config in &self.configs {
            res.extend(config.tags.iter().map(|s| &**s));
        }
        res
    }

    pub fn weight(&self) -> f64 {
        self.configs.last().unwrap().weight
    }

    pub fn ignore(&self) -> bool {
        self.configs.iter().any(|c| c.ignore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::*;
    use std::path::Path;

    #[test]
    fn test_config_for_file() -> Result<()> {
        let config =
            WorkingConfig::new(Config::from_path(Path::new("fixtures/configs/simple.yml"))?);
        assert!(config.config_for_file("schema.rb").is_some());
        assert!(config.config_for_file("spec/models/code_spec.rb").is_some());
        assert!(config.config_for_file("rusty.rs").is_none());
        Ok(())
    }

    #[test]
    fn test_config_for_identity() -> Result<()> {
        let config =
            WorkingConfig::new(Config::from_path(Path::new("fixtures/configs/simple.yml"))?);
        assert!(config
            .config_for_identity(&Identity::parse("Gordon Hempton <ghempton@gmail.com>"))
            .is_some());
        assert!(config
            .config_for_identity(&Identity::parse("Gordon Hempton <gordon@outreach.io>"))
            .is_some());
        assert!(config
            .config_for_identity(&Identity::parse("<ghempton@gmail.com>"))
            .is_none());
        assert!(config
            .config_for_identity(&Identity::parse("Gordon Hempton"))
            .is_none());
        Ok(())
    }

    #[test]
    fn test_base() {
        let config = WorkingConfig::new(Config::base());
        assert!(config.config_for_file("schema.rb").is_some());
        assert!(config.config_for_file("package.json").is_some());
        assert!(config.config_for_file("asdasd.asdasdasd").is_none());
    }

    #[test]
    fn test_overlapping_globs() {
        let mut config = Config::default();
        config.merge_defaults = false;

        config.files.insert(
            "**/*.rb".to_string(),
            GlobConfig {
                weight: 1.0,
                ignore: false,
                tags: vec!["ruby".to_string()],
            },
        );

        config.files.insert(
            "**/*_spec.rb".to_string(),
            GlobConfig {
                weight: 0.5,
                ignore: false,
                tags: vec!["rspec".to_string()],
            },
        );

        config.files.insert(
            "some_bad_spec.rb".to_string(),
            GlobConfig {
                weight: 1.0,
                ignore: true,
                tags: vec![],
            },
        );

        let config = WorkingConfig::new(config);

        let file_config = config.config_for_file("db/schema.rb").unwrap();
        let mut expected_set = HashSet::new();
        expected_set.insert("ruby");
        assert!(file_config.tags() == expected_set);
        assert!(file_config.weight() == 1.0);
        assert!(!file_config.ignore());

        let file_config = config.config_for_file("spec/app_spec.rb").unwrap();
        let mut expected_set = HashSet::new();
        expected_set.insert("ruby");
        expected_set.insert("rspec");
        assert!(file_config.tags() == expected_set);
        assert!(file_config.weight() == 0.5);
        assert!(!file_config.ignore());

        let file_config = config.config_for_file("some_bad_spec.rb").unwrap();
        assert!(file_config.tags() == expected_set);
        assert!(file_config.weight() == 1.0);
        assert!(file_config.ignore());
    }
}
