use std::collections::HashSet;

use linked_hash_map::LinkedHashMap;

use serde_yaml;
use std::fs::File;
use std::path::Path;

use glob::Pattern;

use error::Error;

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
    pub name: Option<String>,

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
    pub fn from_path(path: &Path) -> Result<Self, Error> {
        let file = File::open(path)?;
        Self::from_file(&file)
    }

    pub fn from_file(file: &File) -> Result<Self, Error> {
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
    pub fn from_dir(path: &Path) -> Result<Self, Error> {
        let file_path = path.join(".codealong").join("config.yml");
        let mut config = if file_path.exists() {
            Self::from_path(&file_path)?
        } else {
            Self::base()
        };
        if config.name.is_none() {
            config.name = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_owned());
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

    pub fn config_for_author(&self, _author: &str) -> Option<&AuthorConfig> {
        None
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            name: None,
            github: None,
            merge_defaults: true,
            churn_cutoff: 14,
            files: LinkedHashMap::new(),
            authors: LinkedHashMap::new(),
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
        assert_eq!(config.name, Some("simple".to_owned()));
    }

    #[test]
    fn test_from_dir_with_config() {
        let config = Config::from_dir(Path::new("fixtures/repos/bare_config")).unwrap();
        assert_eq!(config.name, Some("bare_config".to_owned()));
        assert!(config.config_for_file("README.md").is_some());
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
