use std::fs::File;
use std::path::Path;

use linked_hash_map::LinkedHashMap;
use serde_yaml;

use crate::error::*;

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
        Ok(serde_yaml::from_reader::<_, Config>(file)?)
    }

    /// Base config with embedded defaults
    pub fn base() -> Self {
        let mut config = Config::default();
        for file in BASE_CONFIGS.files() {
            config.merge(serde_yaml::from_slice(file.contents()).unwrap());
        }
        config
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
}
