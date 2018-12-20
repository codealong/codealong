use crate::repo::Repo;

/// Given all possible repo-related arguments, expand them to a list of Repo
/// structs.
pub fn expand_repos(matches: &clap::ArgMatches) -> Vec<Repo> {
    let mut repos = Vec::new();

    if let Some(repo_paths) = matches.values_of("repo_path") {
        for repo_path in repo_paths {
            repos.push(Repo::Local(repo_path.to_owned()));
        }
    }

    if let Some(repo_urls) = matches.values_of("repo_url") {
        for repo_url in repo_urls {
            repos.push(Repo::Url(repo_url.to_owned()));
        }
    }

    if let Some(github_orgs) = matches.values_of("github_org") {
        let skip_forks = matches.is_present("skip_forks");
        for github_org in github_orgs {
            repos.append(&mut expand_github_org(github_org, skip_forks));
        }
    }

    repos
}

fn expand_github_org(org: &str, skip_forks: bool) -> Vec<Repo> {
    let url = format!("https://api.github.com/orgs/{}/repos", org);
    let github_client = codealong_github::Client::from_env();
    let cursor: codealong_github::Cursor<codealong_github::Repo> =
        codealong_github::Cursor::new(&github_client, &url);
    cursor
        .filter(|r| !r.fork || !skip_forks)
        .map(|r| Repo::Url(r.html_url))
        .collect()
}
