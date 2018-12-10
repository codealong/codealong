use std::env;

pub fn git_credentials_callback(
    _user: &str,
    _user_from_url: Option<&str>,
    _cred: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    let user = _user_from_url.unwrap_or("git");

    if _cred.contains(git2::CredentialType::USERNAME) {
        return git2::Cred::username(user);
    }

    match env::var("CODEALONG_SSH_KEY") {
        Ok(k) => git2::Cred::ssh_key(user, None, std::path::Path::new(&k), None),
        _ => Err(git2::Error::from_str(
            "unable to get private key from CODEALONG_SSH_KEY",
        )),
    }
}
