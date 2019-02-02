use git2::Repository;
use regex::Regex;
use url::Url;

use crate::error::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoInfo {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub fork: bool,

    #[serde(default)]
    pub github_name: Option<String>,

    // intentionally left as string since Url does not support ssh+git style urls
    #[serde(default)]
    pub clone_url: String,

    #[serde(default)]
    pub refs: Vec<String>,
}

impl RepoInfo {
    pub fn from_url(url: &str) -> Result<RepoInfo> {
        let name = if let Ok(url) = Url::parse(&url) {
            url.path().to_owned().split_off(1)
        } else {
            // We cannot use the url crate to parse ssh urls since they are not
            // standards compliant, e.g. git@github.com:getoutreach/broccoli-babel.git
            lazy_static! {
                static ref SSH_URL_REGEX: Regex =
                    Regex::new(r#"(.+@)?(.+):(?P<path>.+).git"#).unwrap();
            }
            SSH_URL_REGEX
                .captures(url)
                .and_then(|captures| captures.name("path"))
                .ok_or::<Error>(ErrorKind::InvalidRepo(url.to_owned()).into())?
                .as_str()
                .to_owned()
        };

        Ok(RepoInfo {
            name: name.clone(),
            github_name: if url.contains("github") {
                Some(name)
            } else {
                None
            },
            clone_url: url.to_owned(),
            ..Default::default()
        })
    }

    pub fn from_repository(repo: &Repository) -> Result<RepoInfo> {
        let mut repo_info = Self::default();
        if let Ok(remote) = repo.find_remote("origin") {
            let url = remote.url().ok_or::<Error>(
                ErrorKind::InvalidRepo(repo.path().to_string_lossy().to_string()).into(),
            )?;
            repo_info.clone_url = url.to_owned();
            lazy_static! {
                static ref GITHUB_REGEX: Regex = Regex::new(
                    r#"(git@github.com:(?P<a>.+/.+).git)|(https://github.com/(?P<b>.+/.+)(?:.git)?)"#
                )
                .unwrap();
            }
            GITHUB_REGEX.captures(url).map(|captures| {
                repo_info.github_name.replace(
                    captures
                        .name("a")
                        .or_else(|| captures.name("b"))
                        .unwrap()
                        .as_str()
                        .to_owned(),
                );
            });
        }

        if let Some(ref github_name) = repo_info.github_name {
            repo_info.name = github_name.to_owned();
        } else {
            // if we don't have a remote, we just use the name of the containing
            // directory
            repo_info.name = repo
                .path()
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
        }
        Ok(repo_info)
    }

    pub fn partial(&self) -> PartialRepoInfo {
        PartialRepoInfo {
            name: self.name.clone(),
            fork: self.fork,
        }
    }
}

impl Default for RepoInfo {
    fn default() -> Self {
        RepoInfo {
            name: "".to_owned(),
            fork: false,
            github_name: None,
            clone_url: "".to_owned(),
            refs: vec!["refs/remotes/origin/master".to_owned()],
        }
    }
}

/// Subset of RepoInfo that is included with each event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartialRepoInfo {
    name: String,
    fork: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_url() -> Result<()> {
        assert_eq!(
            RepoInfo::from_url("https://github.com/actix/actix-web")?.name,
            "actix/actix-web"
        );
        assert_eq!(
            RepoInfo::from_url("git@github.com:getoutreach/broccoli-babel.git")?.name,
            "getoutreach/broccoli-babel"
        );
        Ok(())
    }
}
