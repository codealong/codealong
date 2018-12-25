use codealong::{AuthorConfig, Config, Identity};

use crate::client::Client;
use crate::cursor::Cursor;
use crate::error::Result;
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
        println!("{:?}", &user);
        add_user_to_config(&mut config, user);
    }
    Ok(config)
}

fn add_user_to_config(config: &mut Config, user: User) {
    let mut author_config = AuthorConfig {
        github_logins: vec![user.login.clone()],
        ..Default::default()
    };

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

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_config_from_org() -> Result<()> {
        let config = config_from_org("rust-lang")?;
        println!("{:?}", config);
        Ok(())
    }
}
