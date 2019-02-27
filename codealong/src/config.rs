use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use linked_hash_map::LinkedHashMap;
use serde_yaml;

use crate::error::*;
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
    pub contributors: Vec<PersonConfig>,
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
        self.contributors.extend(other.contributors);
    }

    /// Merges contributors based on name and email matches. The contributor
    /// higher in the list has precedence.
    pub fn dedup_contributors(&mut self) {
        // This is n^2 and could be more efficient, but the cardinality of
        // contributors is not high...
        let mut index = 0;
        let mut indexes_to_remove = HashSet::new();
        let len = self.contributors.len();
        while index < len {
            let mut next_index = index + 1;
            let (head, tail) = self.contributors.split_at_mut(next_index);
            let curr = head.last_mut().unwrap();
            while next_index < len {
                let next = &tail[next_index - index - 1];
                if curr.person.is_dupe(&next.person) {
                    curr.person.merge(&next.person);
                    indexes_to_remove.insert(next_index);
                }
                next_index += 1;
            }
            index += 1;
        }

        for i in indexes_to_remove {
            self.contributors.remove(i);
        }
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            merge_defaults: true,
            churn_cutoff: 14,
            files: LinkedHashMap::new(),
            contributors: Vec::new(),
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
pub struct PersonConfig {
    #[serde(flatten)]
    pub person: Person,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub ignore: bool,
}

impl Default for PersonConfig {
    fn default() -> PersonConfig {
        PersonConfig {
            person: Default::default(),
            tags: vec![],
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
        assert_eq!(config.contributors.len(), 1);
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
    fn test_dedup_contributors() {
        use crate::identity::Identity;
        let mut config = Config::default();

        config.contributors.push(PersonConfig {
            person: Person {
                id: "a".to_owned(),
                identities: vec![Identity::parse("Gordon Hempton")],
                ..Person::default()
            },
            ..PersonConfig::default()
        });

        config.contributors.push(PersonConfig {
            person: Person {
                id: "b".to_owned(),
                identities: vec![Identity::parse("Gordon Hempton <ghempton@gmail.com>")],
                ..Person::default()
            },
            ..PersonConfig::default()
        });

        config.contributors.push(PersonConfig {
            person: Person {
                id: "c".to_owned(),
                identities: vec![Identity::parse("Someone Else <test@test.com>")],
                ..Person::default()
            },
            ..PersonConfig::default()
        });

        config.dedup_contributors();

        assert_eq!(config.contributors.len(), 2);
        assert_eq!(
            config.contributors.first().as_ref().unwrap().person.id,
            "a".to_owned()
        );
        assert_eq!(
            config
                .contributors
                .first()
                .as_ref()
                .unwrap()
                .person
                .identities,
            vec![
                Identity::parse("Gordon Hempton"),
                Identity::parse("Gordon Hempton <ghempton@gmail.com>")
            ]
        );
    }
}
