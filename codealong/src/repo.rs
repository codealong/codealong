use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, RemoteCallbacks, Repository};
use regex::Regex;

use crate::error::*;
use crate::repo_info::RepoInfo;
use crate::utils::with_authentication;
use crate::workspace::Workspace;

pub type ProgressCallback<'a> = FnMut(usize, usize) + 'a;

pub struct Repo<'workspace> {
    workspace: &'workspace Workspace,
    repo_info: RepoInfo,
}

impl<'workspace> Repo<'workspace> {
    pub fn new(workspace: &'workspace Workspace, repo_info: RepoInfo) -> Repo {
        Repo {
            workspace,
            repo_info,
        }
    }

    fn path(&self) -> PathBuf {
        self.workspace.repo_dir(&self.repo_info)
    }

    pub fn repository(&self) -> Result<Repository> {
        Ok(Repository::discover(self.path())?)
    }

    pub fn init<'a>(&self, mut cb: Option<Box<ProgressCallback<'a>>>) -> Result<Repository> {
        if let Ok(repository) = self.repository() {
            self.fetch(&repository, cb);
            Ok(repository)
        } else {
            self.clone(cb)
        }
    }

    pub fn clone<'a>(&self, mut cb: Option<Box<ProgressCallback<'a>>>) -> Result<Repository> {
        let git_config = git2::Config::open_default()?;
        let url = &self.repo_info.clone_url;
        let into = self.path();
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
}
