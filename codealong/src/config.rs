use std::collections::HashMap;

use serde_yaml;
use std::fs::File;

use glob::Pattern;

use error::Error;

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

    fn default_merge_defaults() -> bool {
        true
    }

    fn default_churn_cutoff() -> u64 {
        14
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
    fn test_config_lookup() {
        let config = Config::from_file("fixtures/configs/simple.yml").unwrap();
        assert!(match config.config_for_file("schema.rb") {
            Some(_) => true,
            _ => false,
        });
        assert!(match config.config_for_file("spec/models/code_spec.rb") {
            Some(_) => true,
            _ => false,
        });
        assert!(match config.config_for_file("rusty.rs") {
            Some(_) => false,
            _ => true,
        });
    }
}
