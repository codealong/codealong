use dirs;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, RemoteCallbacks, Repository};
use url::Url;

use crate::error::Result;
use crate::utils::git_credentials_callback;

#[derive(Debug, Clone, PartialEq)]
pub enum Repo {
    Local(String),
    Url(String, bool),
}

pub type ProgressCallback<'a> = FnMut(usize, usize) + 'a;

impl Repo {
    pub fn init<'a>(&self, cb: Option<Box<ProgressCallback<'a>>>) -> Result<Repository> {
        match self {
            Repo::Local(path) => {
                let repository = Repository::discover(path)?;
                fetch_repo(&repository, cb)?;
                Ok(repository)
            }
            Repo::Url(url, _fork) => {
                let path = clone_destination(url)?;
                match Repository::discover(&path) {
                    Ok(repository) => {
                        fetch_repo(&repository, cb)?;
                        Ok(repository)
                    }
                    Err(_e) => clone_repo(url, &path, cb),
                }
            }
        }
    }

    pub fn repo(&self) -> Result<Repository> {
        match self {
            Repo::Local(path) => Ok(Repository::discover(path)?),
            Repo::Url(url, _fork) => Ok(Repository::discover(clone_destination(&url)?)?),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Repo::Local(path) => &path,
            Repo::Url(url, _fork) => &url,
        }
    }

    pub fn is_fork(&self) -> bool {
        match self {
            Repo::Url(_url, true) => true,
            _ => false,
        }
    }
}

fn fetch_repo<'a>(repo: &Repository, cb: Option<Box<ProgressCallback<'a>>>) -> Result<()> {
    let remote = "origin";
    let mut remote = repo.find_remote(remote)?;

    let mut rcb = RemoteCallbacks::new();
    rcb.credentials(git_credentials_callback);

    if let Some(mut cb) = cb {
        rcb.transfer_progress(move |progress| {
            cb(progress.received_objects(), progress.total_objects());
            true
        });
    }

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(rcb);

    Ok(remote.fetch(&["master"], Some(&mut fo), None)?)
}

fn clone_repo<'a>(
    url: &str,
    into: &Path,
    cb: Option<Box<ProgressCallback<'a>>>,
) -> Result<Repository> {
    let mut rcb = RemoteCallbacks::new();
    rcb.credentials(git_credentials_callback);

    if let Some(mut cb) = cb {
        rcb.transfer_progress(move |progress| {
            cb(progress.received_objects(), progress.total_objects());
            true
        });
    }

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(rcb);

    let co = CheckoutBuilder::new();

    Ok(RepoBuilder::new()
        .fetch_options(fo)
        .with_checkout(co)
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
