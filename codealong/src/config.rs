use std::collections::HashMap;

use git2::Repository;
use serde_yaml;
use std::fs::File;

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
    pub files: HashMap<String, FileConfig>,

    #[serde(default)]
    pub authors: HashMap<String, AuthorConfig>,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Config, Error> {
        let file = File::open(path)?;
        match serde_yaml::from_reader(file) {
            Ok(config) => Ok(config),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Base config with embedded defaults
    pub fn base() -> Config {
        let mut config = Config::default();
        for file in BASE_CONFIGS.files() {
            config.merge(serde_yaml::from_slice(file.contents()).unwrap());
        }
        config
    }

    // pub fn from_repo(repo: &Repository) -> Result<Config, Error> {
    //     let file_path = repo.path();

    // }

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

    pub fn config_for_file(&self, path: &str) -> Option<&FileConfig> {
        self.files
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
            .next()
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
            files: HashMap::new(),
            authors: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileConfig {
    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "FileConfig::default_weight")]
    pub weight: f64,

    #[serde(default)]
    pub ignore: bool,
}

impl FileConfig {
    fn default_weight() -> f64 {
        1.0
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
    fn test_deserialization() {
        let config = Config::from_file("fixtures/configs/simple.yml").unwrap();
        assert_eq!(config.files.len(), 5);
        assert_eq!(config.authors.len(), 1);
    }

    #[test]
    fn test_config_for_file() {
        let config = Config::from_file("fixtures/configs/simple.yml").unwrap();
        assert!(config.config_for_file("schema.rb").is_some());
        assert!(config.config_for_file("spec/models/code_spec.rb").is_some());
        assert!(config.config_for_file("rusty.rs").is_none());
    }

    #[test]
    fn test_merge() {
        let mut config = Config {
            github: None,
            name: None,
            merge_defaults: true,
            churn_cutoff: 14,
            files: HashMap::new(),
            authors: HashMap::new(),
        };

        config.files.insert(
            "**/*.rb".to_string(),
            FileConfig {
                weight: 1.0,
                ignore: false,
                tags: vec!["ruby".to_string()],
            },
        );

        let mut config2 = Config {
            github: None,
            name: None,
            merge_defaults: true,
            churn_cutoff: 14,
            files: HashMap::new(),
            authors: HashMap::new(),
        };

        config2.files.insert(
            "**/*.rs".to_string(),
            FileConfig {
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
}
