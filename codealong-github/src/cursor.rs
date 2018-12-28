use regex::Regex;
use reqwest::header::HeaderMap;

use crate::client::Client;
use crate::error::Result;

/// Provides an iterator on top of the Github pagination API
pub struct Cursor<'client, T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    client: &'client Client,
    next_url: Option<String>,
    num_pages: Option<usize>,
    per_page: Option<usize>,
    current_page: Option<std::vec::IntoIter<T>>,
    has_loaded_page: bool,
}

impl<'client, T> Cursor<'client, T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    pub fn new(client: &'client Client, url: &str) -> Cursor<'client, T> {
        Cursor {
            client,
            next_url: Some(url.to_owned()),
            current_page: None,
            num_pages: None,
            per_page: None,
            has_loaded_page: false,
        }
    }

    pub fn guess_len(&mut self) -> Option<usize> {
        self.ensure_page_loaded();
        self.num_pages
            .and_then(|num_page| self.per_page.map(|per_page| num_page * per_page))
    }

    fn get_next_url(&self, headers: &HeaderMap) -> Option<String> {
        let link = headers.get("link");
        link.and_then(|link| {
            lazy_static! {
                static ref LINK_NEXT_REGEX: Regex = Regex::new(r#"<([^ ]*)>; rel="next""#).unwrap();
            }
            LINK_NEXT_REGEX
                .captures(link.to_str().unwrap())
                .map(|captures| captures[1].to_owned())
        })
    }

    fn read_from_current_page(&mut self) -> Option<T> {
        self.current_page.as_mut().and_then(|iter| iter.next())
    }

    fn get_num_pages(&self, headers: &HeaderMap) -> Option<usize> {
        let link = headers.get("link");
        link.and_then(|link| {
            lazy_static! {
                static ref LINK_LAST_PAGE_REGEX: Regex =
                    Regex::new(r#"<[^ ]*page=(\d+)[^ ]*>; rel="last""#).unwrap();
            }
            LINK_LAST_PAGE_REGEX
                .captures(link.to_str().unwrap())
                .map(|captures| captures[1].to_owned().parse::<usize>().unwrap())
        })
    }

    fn ensure_page_loaded(&mut self) {
        if !self.has_loaded_page {
            self.load_next_page();
        }
    }

    fn load_next_page(&mut self) -> Result<()> {
        if let Some(next_url) = self.next_url.take() {
            let mut res = self.client.get(&next_url)?;
            self.has_loaded_page = true;
            let new_page = res.json::<Vec<T>>().unwrap().into_iter();
            let headers = res.headers();
            self.next_url = self.get_next_url(&headers);
            if let None = self.num_pages {
                self.num_pages = self.get_num_pages(&headers);
            }
            if let None = self.per_page {
                self.per_page = Some(new_page.len());
            }
            self.current_page = Some(new_page);
            Ok(())
        } else {
            Ok(())
        }
    }
}

impl<'client, T> Iterator for Cursor<'client, T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.read_from_current_page().or_else(|| {
            self.load_next_page();
            self.read_from_current_page()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pull_request::PullRequest;

    #[test]
    fn test_cursor() {
        let client = Client::from_env();
        let mut cursor: Cursor<PullRequest> = Cursor::new(
            &client,
            "https://api.github.com/repos/facebook/react/pulls?state=all",
        );
        assert!(cursor.guess_len().unwrap() > 100);
        assert_eq!(cursor.take(100).collect::<Vec<PullRequest>>().len(), 100);
    }
}
