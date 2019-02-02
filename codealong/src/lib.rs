#![recursion_limit = "4096"]

extern crate chrono;
#[macro_use]
extern crate error_chain;
extern crate git2;
extern crate glob;
#[macro_use]
extern crate include_dir;
extern crate regex;
#[macro_use]
extern crate lazy_static;
extern crate linked_hash_map;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_yaml;
#[macro_use]
extern crate slog;

mod analyze_opts;
mod analyzed_commit;
mod analyzed_diff;
mod commit_analyzer;
mod config;
mod config_context;
mod diff_analyzer;
mod error;
mod event;
mod fast_blame;
mod file_analyzer;
mod hunk_analyzer;
mod identity;
mod line_analyzer;
mod person;
mod repo;
mod repo_analyzer;
mod repo_config;
mod repo_info;
pub mod test;
mod utils;
mod work_stats;
mod workspace;
mod workspace_config;

pub use crate::analyze_opts::AnalyzeOpts;
pub use crate::analyzed_commit::AnalyzedCommit;
pub use crate::analyzed_diff::AnalyzedDiff;
pub use crate::commit_analyzer::CommitAnalyzer;
pub use crate::config::{AuthorConfig, Config, GlobConfig};
pub use crate::diff_analyzer::DiffAnalyzer;
pub use crate::error::{Error, ErrorKind};
pub use crate::event::Event;
pub use crate::identity::Identity;
pub use crate::person::Person;
pub use crate::repo::Repo;
pub use crate::repo_analyzer::{RepoAnalyzer, AnalyzedRevwalk};
pub use crate::repo_config::RepoConfig;
pub use crate::repo_info::RepoInfo;
pub use crate::utils::with_authentication;
pub use crate::workspace::Workspace;
pub use crate::workspace_config::{RepoEntry, WorkspaceConfig};
