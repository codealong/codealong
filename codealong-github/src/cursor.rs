use client::Client;
use regex::Regex;
use reqwest::header::HeaderMap;

/// Provides an iterator on top of the Github pagination API
pub struct Cursor<'client, T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    client: &'client Client,
    next_url: Option<String>,
    current_page: Option<std::vec::IntoIter<T>>,
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
        }
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
}

impl<'client, T> Iterator for Cursor<'client, T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        let v = self.current_page.as_mut().and_then(|iter| iter.next());

        if let None = v {
            self.next_url.take().map(|next_url| {
                let mut res = self.client.build_request(&next_url).send().unwrap();
                let new_page = res.json::<Vec<T>>().unwrap().into_iter();
                let headers = res.headers();
                self.next_url = self.get_next_url(&headers);
                self.current_page = Some(new_page);
            });
            self.current_page.as_mut().and_then(|iter| iter.next())
        } else {
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pull_request::PullRequest;

    #[test]
    fn test_cursor() {
        let client = Client::public();
        let cursor: Cursor<PullRequest> = Cursor::new(
            &client,
            "https://api.github.com/repos/facebook/react/pulls?state=all",
        );
        assert_eq!(cursor.take(100).collect::<Vec<PullRequest>>().len(), 100);
    }
}
