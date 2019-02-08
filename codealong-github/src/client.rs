use reqwest;
use reqwest::Response;
use std::env;

use crate::error::{ErrorKind, ErrorPayload, Result};

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

    pub fn get(&self, url: &str) -> Result<Response> {
        self.get_with_content_type(url, "application/vnd.github+json")
    }

    pub fn get_with_content_type(&self, url: &str, content_type: &str) -> Result<Response> {
        let client = reqwest::Client::new();
        let mut builder = client.get(url).header("Accept", content_type);
        if let Some(ref token) = self.token {
            builder = builder.header("Authorization", format!("token {}", token));
        }
        let mut res = builder.send()?;
        if res.status().is_success() {
            Ok(res)
        } else {
            Err(self.get_error_kind(&mut res).into())
        }
    }

    fn get_error_kind(&self, res: &mut reqwest::Response) -> ErrorKind {
        let payload = res.json::<ErrorPayload>().unwrap();
        if payload
            .message
            .contains("have triggered an abuse detection mechanism")
        {
            ErrorKind::RateLimitted
        } else {
            ErrorKind::Unknown
        }
    }
}
