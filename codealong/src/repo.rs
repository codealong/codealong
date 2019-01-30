use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, RemoteCallbacks, Repository};
use regex::Regex;

use crate::config::Config;
use crate::error::*;
use crate::repo_info::RepoInfo;
use crate::utils::with_authentication;

pub type ProgressCallback<'a> = FnMut(usize, usize) + 'a;

#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    base_config: Config,
    path: PathBuf,
    repo_info: RepoInfo,
}

impl Repo {
    pub fn new(base_config: Config, path: PathBuf, repo_info: RepoInfo) -> Repo {
        Repo {
            base_config,
            path,
            repo_info,
        }
    }

    pub fn repository(&self) -> Result<Repository> {
        Ok(Repository::discover(&self.path)?)
    }

    pub fn init<'a>(&self, mut cb: Option<Box<ProgressCallback<'a>>>) -> Result<Repository> {
        if let Ok(repository) = self.repository() {
            self.fetch(&repository, cb)?;
            Ok(repository)
        } else {
            self.clone_repo(cb)
        }
    }

    pub fn clone_repo<'a>(&self, mut cb: Option<Box<ProgressCallback<'a>>>) -> Result<Repository> {
        let git_config = git2::Config::open_default()?;
        let url = &self.repo_info.clone_url;
        let into = &self.path;
        Ok(with_authentication(url, &git_config, |f| {
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
                .bare(true)
                .fetch_options(fo)
                .with_checkout(co)
                .clone(url, &into)?)
        })?)
    }

    pub fn fetch<'a>(
        &self,
        repository: &Repository,
        mut cb: Option<Box<ProgressCallback<'a>>>,
    ) -> Result<()> {
        let remote = "origin";
        let mut remote = repository.find_remote(remote)?;
        let git_config = git2::Config::open_default()?;
        let url = remote.url().unwrap().to_owned();
        Ok(with_authentication(&url, &git_config, move |f| {
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

            Ok(remote.fetch(
                &self.repo_info.refs.iter().map(|s| &**s).collect::<Vec<_>>()[..],
                Some(&mut fo),
                None,
            )?)
        })?)
    }

    pub fn repo_info(&self) -> &RepoInfo {
        &self.repo_info
    }

    /// Combines base config with any config found in the repo itself
    pub fn config(&self) -> Config {
        // TODO once we go to bare repos we need to
        // read the object directly from git
        self.base_config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone() -> Result<()> {
        let config = Config::default();
        let tmp_dir = tempdir::TempDir::new("example")?;
        let repo_info = RepoInfo {
            name: "simple".to_owned(),
            clone_url: "./fixtures/repos/simple".to_owned(),
            ..Default::default()
        };
        let repo = Repo::new(config, tmp_dir.path().join("simple.git"), repo_info);
        repo.clone_repo(None)?;
        Ok(())
    }
}
