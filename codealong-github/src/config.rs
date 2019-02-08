use slog::Logger;
use std::collections::HashMap;

use codealong::{AuthorConfig, Config, Identity, RepoEntry, RepoInfo, WorkspaceConfig};

use crate::client::Client;
use crate::cursor::Cursor;
use crate::error::{retry_when_rate_limited, Result};
use crate::repo::Repo;
use crate::team::Team;
use crate::user::User;

/// Generate a `Config` from Github organization information. The primary use
/// case here is to generate a list of authors with aliases from all of the
/// organization's members.
pub fn config_from_org(
    client: &Client,
    github_org: &str,
    logger: &Logger,
) -> Result<WorkspaceConfig> {
    let config = default_config_with_authors(client, github_org, logger)?;
    let repos = build_repo_entries(client, github_org, logger)?;
    Ok(WorkspaceConfig { config, repos })
}

fn default_config_with_authors(
    client: &Client,
    github_org: &str,
    logger: &Logger,
) -> Result<Config> {
    let all_teams = get_all_teams(client, github_org, logger)?;
    let url = format!("https://api.github.com/orgs/{}/members", github_org);
    let cursor: Cursor<User> = Cursor::new(&client, &url);
    let mut config = Config::default();
    for user in cursor {
        let teams = all_teams.get(&user.login);
        add_user_to_config(&client, &mut config, user, teams, logger)?;
    }
    Ok(config)
}

fn get_all_teams(
    client: &Client,
    github_org: &str,
    logger: &Logger,
) -> Result<HashMap<String, Vec<Team>>> {
    let url = format!("https://api.github.com/orgs/{}/teams", github_org);
    let cursor: Cursor<Team> = Cursor::new(&client, &url);
    let mut res: HashMap<String, Vec<Team>> = HashMap::new();
    for team in cursor {
        let url = format!("https://api.github.com/teams/{}/members", &team.id);
        let cursor: Cursor<User> = Cursor::new(&client, &url);
        for user in cursor {
            let teams = res.entry(user.login).or_insert_with(|| Vec::new());
            teams.push(team.clone());
        }
    }
    Ok(res)
}

fn add_user_to_config(
    client: &Client,
    config: &mut Config,
    mut user: User,
    teams: Option<&Vec<Team>>,
    logger: &Logger,
) -> Result<()> {
    augment_with_search_data(client, &mut user, logger)?;

    let formatted_teams = teams
        .map(|teams| teams.iter().map(|team| team.name.clone()).collect())
        .unwrap_or_else(|| Vec::new());

    let author_config = AuthorConfig {
        github_logins: vec![user.login.clone()],
        teams: formatted_teams,
        ..Default::default()
    };

    // Prefer a User <email> formatted id for the author, but fallback to using
    // the github login
    let key = if user.email.is_some() || user.name.is_some() {
        Identity {
            name: user.name,
            email: user.email,
        }
        .to_string()
    } else {
        user.login
    };

    config.authors.insert(key, author_config);
    Ok(())
}

// Use the github search API to attempt to get email/name directly from commits
fn augment_with_search_data(client: &Client, user: &mut User, logger: &Logger) -> Result<()> {
    let url = format!(
        "https://api.github.com/search/commits?q=author:{}",
        &user.login
    );
    let mut resp = retry_when_rate_limited(
        &mut || client.get_with_content_type(&url, "application/vnd.github.cloak-preview"),
        Some(&mut |seconds| warn!(logger, "Rate limit reached, sleeping {} seconds", seconds)),
    )?;
    let results = resp.json::<SearchResults>()?;

    if let Some(r) = results.items.first() {
        user.email = user.email.take().or_else(|| r.commit.author.email.clone());
        user.name = user.name.take().or_else(|| r.commit.author.name.clone());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SearchResults {
    items: Vec<SearchResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SearchResult {
    commit: Commit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Commit {
    author: Signature,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Signature {
    pub name: Option<String>,
    pub email: Option<String>,
}

fn build_repo_entries(
    client: &Client,
    github_org: &str,
    logger: &Logger,
) -> Result<Vec<RepoEntry>> {
    let url = format!("https://api.github.com/orgs/{}/repos", github_org);
    let cursor: Cursor<Repo> = Cursor::new(&client, &url);
    let res = cursor.map(|repo| RepoEntry {
        repo_info: RepoInfo {
            name: repo.full_name.clone(),
            github_name: Some(repo.full_name.clone()),
            clone_url: repo.ssh_url,
            fork: repo.fork,
            ..Default::default()
        },
        path: Some(format!("{}.git", repo.full_name)),
        ignore: false,
    });
    Ok(res.collect())
}

#[cfg(test)]
mod test {
    use super::*;
    use codealong::test::build_test_logger;

    #[test]
    fn test_config_from_org() -> Result<()> {
        let client = Client::from_env();
        let workspace_config = config_from_org(&client, "codealong", &build_test_logger())?;
        assert!(workspace_config.config.authors.len() >= 1);
        assert!(workspace_config.repos.len() >= 1);
        assert_eq!(
            workspace_config
                .config
                .authors
                .iter()
                .next()
                .unwrap()
                .1
                .tags,
            vec!["team:Devs".to_owned(), "team:Ninjas".to_owned()]
        );
        Ok(())
    }
}
