use reqwest;

error_chain! {
    foreign_links {
        ES(reqwest::Error);
    }
}
