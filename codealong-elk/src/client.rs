use chrono::prelude::*;
use chrono::DateTime;

use event::Event;

use error::Result;
use reqwest;

pub struct Client {
    url: String,
}

impl Default for Client {
    fn default() -> Self {
        Client {
            url: "http://localhost:9200".to_owned(),
        }
    }
}

impl Client {
    pub fn new(url: &str) -> Client {
        Client {
            url: url.to_owned(),
        }
    }

    pub fn index<T: codealong::Event + serde::Serialize>(
        &self,
        event: T,
    ) -> Result<reqwest::Response> {
        let event = Event::new(event);
        let client = reqwest::Client::new();
        let index = get_es_index(event.timestamp());
        let url = format!("{}/{}/_doc/{}", self.url, index, event.id());
        Ok(client.put(&url).json(&event).send()?)
    }
}

fn get_es_index(date: &DateTime<Utc>) -> String {
    date.format("codealong-%Y.%m").to_string()
}
