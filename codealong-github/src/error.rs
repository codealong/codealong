use codealong;
use git2;
use reqwest;

error_chain! {
    errors {
        RateLimitted {}
        Unknown {}
    }

    foreign_links {
        Git2(git2::Error);
        IO(std::io::Error);
        Reqwest(reqwest::Error);
    }

    links {
        Core(codealong::Error, codealong::ErrorKind);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub message: String,
    pub documentation_url: Option<String>,
}

const MAX_RETRY: u64 = 4;
const RETRY_INTERVAL_SECONDS: u64 = 60;

pub fn retry_when_rate_limited(
    f: &mut FnMut() -> Result<reqwest::Response>,
    mut cb: Option<&mut FnMut(u64)>,
) -> Result<reqwest::Response> {
    for _ in 0..MAX_RETRY {
        match f() {
            Err(Error(ErrorKind::RateLimitted, _)) => {
                if let Some(ref mut cb) = cb {
                    cb(RETRY_INTERVAL_SECONDS);
                }
                std::thread::sleep(std::time::Duration::new(RETRY_INTERVAL_SECONDS, 0));
            }
            r => return r,
        }
    }
    Err(ErrorKind::RateLimitted.into())
}
