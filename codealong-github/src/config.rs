use codealong::{AuthorConfig, Config, Identity};

use crate::client::Client;
use crate::cursor::Cursor;
use crate::error::{retry_when_rate_limited, Result};
use crate::user::User;

/// Generate a `Config` from Github organization information. The primary use
/// case here is to generate a list of authors with aliases from all of the
/// organization's members.
pub fn config_from_org(github_org: &str) -> Result<Config> {
    let client = Client::from_env();
    let url = format!("https://api.github.com/orgs/{}/members", github_org);
    let cursor: Cursor<User> = Cursor::new(&client, &url);
    let mut config = Config::default();
    for user in cursor {
        add_user_to_config(&client, &mut config, user);
    }
    Ok(config)
}

fn add_user_to_config(client: &Client, config: &mut Config, mut user: User) {
    let mut author_config = AuthorConfig {
        github_logins: vec![user.login.clone()],
        ..Default::default()
    };

    augment_with_search_data(client, &mut user);

    if user.email.is_some() || user.name.is_some() {
        author_config.aliases.push(
            Identity {
                name: user.name,
                email: user.email,
            }
            .to_string(),
        );
    }

    config.authors.insert(user.login, author_config);
}

// Use the github search API to attempt to get email/name directly from commits
fn augment_with_search_data(client: &Client, user: &mut User) -> Result<()> {
    let url = format!(
        "https://api.github.com/search/commits?q=author:{}",
        &user.login
    );
    let mut resp = retry_when_rate_limited(
        &mut || client.get_with_content_type(&url, "application/vnd.github.cloak-preview"),
        Some(&mut |seconds| println!("Rate limit reached, sleeping {} seconds", seconds)),
    )?;
    let results = resp.json::<SearchResults>()?;

    if let [r, ..] = results.items.as_slice() {
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

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_config_from_org() -> Result<()> {
        let config = config_from_org("serde-rs")?;
        assert!(config.authors.len() >= 3);
        Ok(())
    }
}
