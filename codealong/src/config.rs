use std::collections::HashSet;
use std::fs::File;
use std::iter;
use std::path::Path;

use glob::Pattern;
use linked_hash_map::LinkedHashMap;
use serde_yaml;

use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::person::Person;

use include_dir::Dir;

static BASE_CONFIGS: Dir = include_dir!("./config");

/// Understands the `.codealong/config.yml` file format.
///
/// Example configuration:
///
/// ```yaml
/// github: ghempton/codealong
/// churn_cutoff: 14
///
/// merge_defaults: true
///
/// files:
///   "**/*.rb":
///     tags:
///       - "ruby"
///   "cassettes/**/*.yml":
///     ignore: true
///   "spec/**/*_spec.rb":
///     tags:
///       - "ruby"
///       - "rspec"
///       - "test"
///   "**/*.css":
///     tags:
///       - "styles"
///       - "css"
///     weight: 0.5
///
/// authors:
///   "Gordon Hempton <ghempton@gmail.com>":
///     aliases:
///       - "Gordon Hempton <gordon@hempton.com>"
///     tags:
///       - "team-apollo"
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "Config::default_merge_defaults")]
    pub merge_defaults: bool,

    #[serde(default = "Config::default_churn_cutoff")]
    pub churn_cutoff: u64,

    #[serde(default)]
    pub files: LinkedHashMap<String, GlobConfig>,

    #[serde(default)]
    pub authors: LinkedHashMap<String, AuthorConfig>,
}

impl Config {
    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        Self::from_file(&file)
    }

    pub fn from_file(file: &File) -> Result<Self> {
        match serde_yaml::from_reader::<_, Config>(file) {
            Ok(mut config) => {
                config.maybe_apply_base();
                Ok(config)
            }
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Base config with embedded defaults
    pub fn base() -> Self {
        let mut config = Config::default();
        for file in BASE_CONFIGS.files() {
            config.merge(serde_yaml::from_slice(file.contents()).unwrap());
        }
        config
    }

    pub fn maybe_apply_base(&mut self) {
        if self.merge_defaults {
            self.merge(Self::base());
        }
    }

    fn default_merge_defaults() -> bool {
        true
    }

    fn default_churn_cutoff() -> u64 {
        14
    }

    /// Merges in all file and author configs
    pub fn merge(&mut self, other: Config) {
        self.files.extend(other.files);
        self.authors.extend(other.authors);
    }

    pub fn config_for_file(&self, path: &str) -> Option<FileConfig> {
        let glob_configs: Vec<&GlobConfig> = self
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
        for (key, author_config) in &self.authors {
            for alias in iter::once(key).chain(&author_config.aliases) {
                if &Identity::parse(alias) == identity {
                    return Some(PersonConfig::new(key, author_config));
                }
            }
        }
        None
    }

    pub fn config_for_github_login(&self, github_login: &str) -> Option<PersonConfig> {
        for (key, author_config) in &self.authors {
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

impl Default for Config {
    fn default() -> Config {
        Config {
            merge_defaults: true,
            churn_cutoff: 14,
            files: LinkedHashMap::new(),
            authors: LinkedHashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobConfig {
    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "GlobConfig::default_weight")]
    pub weight: f64,

    #[serde(default)]
    pub ignore: bool,
}

impl GlobConfig {
    fn default_weight() -> f64 {
        1.0
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorConfig {
    #[serde(default)]
    pub aliases: Vec<String>,

    #[serde(default)]
    pub github_logins: Vec<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub teams: Vec<String>,

    #[serde(default)]
    pub ignore: bool,
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

impl Default for AuthorConfig {
    fn default() -> AuthorConfig {
        AuthorConfig {
            aliases: vec![],
            github_logins: vec![],
            tags: vec![],
            teams: vec![],
            ignore: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialization() {
        let config = Config::from_path(Path::new("fixtures/configs/simple.yml")).unwrap();
        assert_eq!(config.files.len(), 5);
        assert_eq!(config.authors.len(), 1);
    }

    #[test]
    fn test_config_for_file() {
        let config = Config::from_path(Path::new("fixtures/configs/simple.yml")).unwrap();
        assert!(config.config_for_file("schema.rb").is_some());
        assert!(config.config_for_file("spec/models/code_spec.rb").is_some());
        assert!(config.config_for_file("rusty.rs").is_none());
    }

    #[test]
    fn test_config_for_identity() {
        let config = Config::from_path(Path::new("fixtures/configs/simple.yml")).unwrap();
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
    }

    #[test]
    fn test_merge() {
        let mut config = Config::default();

        config.files.insert(
            "**/*.rb".to_string(),
            GlobConfig {
                weight: 1.0,
                ignore: false,
                tags: vec!["ruby".to_string()],
            },
        );

        let mut config2 = Config::default();

        config2.files.insert(
            "**/*.rs".to_string(),
            GlobConfig {
                weight: 1.0,
                ignore: false,
                tags: vec!["rust".to_string()],
            },
        );

        config.merge(config2);

        assert!(config.files.keys().len() == 2);
    }

    #[test]
    fn test_base() {
        let config = Config::base();
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
