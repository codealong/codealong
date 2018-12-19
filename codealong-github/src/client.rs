use reqwest;
use reqwest::RequestBuilder;
use std::env;

pub struct Client {
    token: Option<String>,
}

/// Very basic wrapper around reqwest to interact with the github api
impl Client {
    pub fn new(token: String) -> Client {
        Client { token: Some(token) }
    }

    pub fn public() -> Client {
        Client { token: None }
    }

    pub fn from_env() -> Client {
        Client {
            token: env::var_os("GITHUB_TOKEN").and_then(|s| s.into_string().ok()),
        }
    }

    pub fn build_request(&self, url: &str) -> RequestBuilder {
        let client = reqwest::Client::new();
        let mut builder = client
            .get(url)
            .header("Accept", "application/vnd.github+json");
        if let Some(ref token) = self.token {
            builder = builder.header("Authorization", format!("token {}", token));
        }
        builder
    }
}
