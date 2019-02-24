use glob::Pattern;
use std::collections::HashSet;
use std::iter;
use std::path::Path;

use crate::config::{AuthorConfig, Config, GlobConfig};
use crate::error::*;
use crate::identity::Identity;
use crate::person::Person;

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

    pub fn config_for_identity(&self, identity: &Identity) -> Option<PersonConfig> {
        let config = &self.config;
        for (key, author_config) in &config.authors {
            for alias in iter::once(key).chain(&author_config.aliases) {
                if &Identity::parse(alias) == identity {
                    return Some(PersonConfig::new(key, author_config));
                }
            }
        }
        None
    }

    pub fn config_for_github_login(&self, github_login: &str) -> Option<PersonConfig> {
        let config = &self.config;
        for (key, author_config) in &config.authors {
            for login in &author_config.github_logins {
                if login == github_login {
                    return Some(PersonConfig::new(key, author_config));
                }
            }
        }
        None
    }

    pub fn person_for_identity(&self, identity: &Identity) -> Person {
        if let Some(person_config) = self.config_for_identity(identity) {
            person_config.to_person()
        } else {
            identity.to_person()
        }
    }

    pub fn person_for_github_login(&self, github_login: &str) -> Person {
        if let Some(person_config) = self.config_for_github_login(github_login) {
            person_config.to_person()
        } else {
            Person {
                id: github_login.to_owned(),
                github_login: Some(github_login.to_owned()),
                name: None,
                email: None,
                teams: vec![],
            }
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

pub struct PersonConfig<'a> {
    key: &'a str,
    config: &'a AuthorConfig,
}

impl<'a> PersonConfig<'a> {
    pub fn new(key: &'a str, config: &'a AuthorConfig) -> PersonConfig<'a> {
        PersonConfig { key, config }
    }

    pub fn tags(&self) -> &'a Vec<String> {
        &self.config.tags
    }

    pub fn ignore(&self) -> bool {
        self.config.ignore
    }

    pub fn to_person(&self) -> Person {
        let id = Identity::parse(self.key);
        Person {
            id: self.key.to_owned(),
            name: id.name,
            email: id.email,
            github_login: self.config.github_logins.first().map(|s| s.to_owned()),
            teams: self.config.teams.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .is_some());
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
