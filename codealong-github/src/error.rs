use codealong;
use git2;
use reqwest;

error_chain! {
    errors { RateLimitted }

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
}
