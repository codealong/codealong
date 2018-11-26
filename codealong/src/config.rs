use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use git2::Repository;
use glob::Pattern;
use linked_hash_map::LinkedHashMap;
use regex::Regex;
use serde_yaml;

use error::{Error, Result};

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
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub github: Option<String>,

    #[serde(default)]
    pub repo_name: Option<String>,

    #[serde(default = "Config::default_merge_defaults")]
    pub merge_defaults: bool,

    #[serde(default = "Config::default_churn_cutoff")]
    pub churn_cutoff: u64,

    #[serde(default)]
    pub files: LinkedHashMap<String, GlobConfig>,

    #[serde(default)]
    pub authors: LinkedHashMap<String, AuthorConfig>,

    #[serde(default, skip_deserializing, skip_serializing)]
    alias_cache: RefCell<Option<HashMap<String, String>>>,
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

    /// Attempts to read the config from the conventional location within the
    /// directory at `.codealong/config.yml`. If no config is found, fallback
    /// to the base config.
    ///
    /// If the config has no `name`, then default to the name of the directory.
    pub fn from_dir(path: &Path) -> Result<Self> {
        let file_path = path.join(".codealong").join("config.yml");
        let mut config = if file_path.exists() {
            let mut config = Self::from_path(&file_path)?;
            config.maybe_apply_base();
            config
        } else {
            Self::base()
        };
        if config.repo_name.is_none() {
            config.repo_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_owned());
        }
        Ok(config)
    }

    pub fn from_repo(repo: &Repository) -> Result<Self> {
        let mut config = Self::from_dir(repo.path())?;
        // attempt to infer a github value based off of origin
        if let Ok(remote) = repo.find_remote("origin") {
            if let Some(url) = remote.url() {
                lazy_static! {
                    static ref GITHUB_REGEX: Regex =
                        Regex::new(r#"git@github.com:(.+/.+).git"#).unwrap();
                }
                GITHUB_REGEX
                    .captures(url)
                    .map(|captures| config.github.replace(captures[1].to_owned()));
            }
        }
        Ok(config)
    }

    /// Base config with embedded defaults
    pub fn base() -> Self {
        let mut config = Config::default();
        for file in BASE_CONFIGS.files() {
            config.merge(serde_yaml::from_slice(file.contents()).unwrap());
        }
        config
    }

    fn maybe_apply_base(&mut self) {
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

    fn ensure_alias_cache(&self) {
        let mut cache = self.alias_cache.borrow_mut();
        if cache.is_none() {
            *cache = Some(self.build_alias_cache());
        }
    }

    fn build_alias_cache(&self) -> HashMap<String, String> {
        let mut res = HashMap::new();
        for (author, config) in self.authors.iter() {
            for alias in config.aliases.iter() {
                res.insert(alias.clone(), author.to_owned());
            }
        }
        res
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

    pub fn config_for_author(&self, author: &str) -> Option<&AuthorConfig> {
        let author = author.to_owned();
        self.ensure_alias_cache();
        let alias_cache = self.alias_cache.borrow();
        let normalized_author = alias_cache
            .as_ref()
            .unwrap()
            .get(&author)
            .unwrap_or(&author);
        self.authors.get(normalized_author)
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            repo_name: None,
            github: None,
            merge_defaults: true,
            churn_cutoff: 14,
            files: LinkedHashMap::new(),
            authors: LinkedHashMap::new(),
            alias_cache: RefCell::new(None),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthorConfig {
    #[serde(default)]
    pub aliases: Vec<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub ignore: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_dir_without_config() {
        let config = Config::from_dir(Path::new("fixtures/repos/simple")).unwrap();
        assert_eq!(config.repo_name, Some("simple".to_owned()));
    }

    #[test]
    fn test_from_dir_with_config() {
        let config = Config::from_dir(Path::new("fixtures/repos/bare_config")).unwrap();
        assert_eq!(config.repo_name, Some("bare_config".to_owned()));
        assert!(config.config_for_file("README.md").is_some());
    }

    #[test]
    fn test_from_repo() {
        let config =
            Config::from_repo(&Repository::open("fixtures/repos/bare_config").unwrap()).unwrap();
        assert_eq!(config.github, None);
        let config = Config::from_repo(&Repository::open_from_env().unwrap()).unwrap();
        assert_eq!(config.github, Some("ghempton/codealong".to_owned()));
    }

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
    fn test_config_for_author() {
        let config = Config::from_path(Path::new("fixtures/configs/simple.yml")).unwrap();
        assert!(config
            .config_for_author("Gordon Hempton <ghempton@gmail.com>")
            .is_some());
        assert!(config
            .config_for_author("Gordon Hempton <gordon@outreach.io>")
            .is_some());
        assert!(config.config_for_author("<ghempton@gmail.com>").is_some());
        assert!(config.config_for_author("Gordon Hempton").is_none());
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
