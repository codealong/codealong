use dirs;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, Progress, RemoteCallbacks, Repository};
use url::Url;

use crate::error::Result;
use crate::utils::git_credentials_callback;

#[derive(Debug, Clone, PartialEq)]
pub enum Repo {
    Local(String),
    Url(String),
}

impl Repo {
    pub fn init(&self) -> Result<Repository> {
        match self {
            Repo::Local(path) => Ok(Repository::discover(path)?),
            Repo::Url(url) => ensure_repo_exists(url),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Repo::Local(path) => &path,
            Repo::Url(url) => &url,
        }
    }
}

fn ensure_repo_exists(url: &str) -> Result<Repository> {
    let path = clone_destination(url)?;
    match Repository::discover(path.as_path()) {
        Err(_) => clone_repo(url, path.as_path()),
        Ok(r) => Ok(r),
    }
}

fn clone_repo(url: &str, into: &Path) -> Result<Repository> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(git_credentials_callback);

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);

    //let mut co = CheckoutBuilder::new();

    Ok(RepoBuilder::new()
        .fetch_options(fo)
        //.with_checkout(co)
        .clone(url, into)?)
}

fn repo_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".codealong")
}

fn clone_destination(url: &str) -> Result<PathBuf> {
    let repo_dir = repo_dir();
    create_dir_all(&repo_dir)?;
    let url = Url::parse(url)?;
    let dir_name = url.path_segments().unwrap().last().unwrap();
    Ok(repo_dir.join(dir_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_destination() -> Result<()> {
        assert_eq!(
            clone_destination("https://github.com/actix/actix-web")?,
            dirs::home_dir()
                .unwrap()
                .join(".codealong")
                .join("actix-web")
        );
        Ok(())
    }
}
