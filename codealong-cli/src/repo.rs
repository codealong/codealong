use dirs;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, RemoteCallbacks, Repository};
use regex::Regex;
use url::Url;

use crate::error::*;
use crate::utils::with_authentication;

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
                create_dir_all(&path)?;
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

fn fetch_repo<'a>(repo: &Repository, mut cb: Option<Box<ProgressCallback<'a>>>) -> Result<()> {
    let remote = "origin";
    let mut remote = repo.find_remote(remote)?;
    let git_config = git2::Config::open_default()?;
    let url = remote.url().unwrap().to_owned();
    with_authentication(&url, &git_config, move |f| {
        let mut rcb = RemoteCallbacks::new();
        rcb.credentials(f);

        if let Some(ref mut cb) = cb {
            rcb.transfer_progress(move |progress| {
                cb(progress.received_objects(), progress.total_objects());
                true
            });
        }

        let mut fo = FetchOptions::new();
        fo.remote_callbacks(rcb);

        Ok(remote.fetch(&["master"], Some(&mut fo), None)?)
    })
}

fn clone_repo<'a>(
    url: &str,
    into: &Path,
    mut cb: Option<Box<ProgressCallback<'a>>>,
) -> Result<Repository> {
    let git_config = git2::Config::open_default()?;
    with_authentication(url, &git_config, |f| {
        let mut rcb = RemoteCallbacks::new();
        rcb.credentials(f);

        if let Some(ref mut cb) = cb {
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
    })
}

fn repo_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".codealong")
}

fn clone_destination(url: &str) -> Result<PathBuf> {
    let repo_dir = repo_dir();
    let dir_name = if let Ok(url) = Url::parse(url) {
        url.path().to_owned().split_off(1)
    } else {
        // We cannot use the url crate to parse ssh urls since they are not
        // standards compliant, e.g. git@github.com:getoutreach/broccoli-babel.git
        lazy_static! {
            static ref SSH_URL_REGEX: Regex = Regex::new(r#"(.+@)?(.+):(?P<path>.+).git"#).unwrap();
        }
        SSH_URL_REGEX
            .captures(url)
            .and_then(|captures| captures.name("path"))
            .ok_or::<Error>(ErrorKind::InvalidRepo(url.to_owned()).into())?
            .as_str()
            .to_owned()
    };
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
                .join("actix")
                .join("actix-web")
        );
        assert_eq!(
            clone_destination("git@github.com:getoutreach/broccoli-babel.git")?,
            dirs::home_dir()
                .unwrap()
                .join(".codealong")
                .join("getoutreach")
                .join("broccoli-babel")
        );
        Ok(())
    }
}
