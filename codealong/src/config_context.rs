use config::{AuthorConfig, FileConfig};

pub struct ConfigContext {
    tags: Vec<String>,
    weight: f64,
}

/// During analyzation, this struct stores the current applicable config.
impl ConfigContext {
    pub fn new(
        file_config: Option<&FileConfig>,
        author_config: Option<&AuthorConfig>,
    ) -> ConfigContext {
        let weight = file_config.map(|c| c.weight()).unwrap_or(1.0);
        let mut tags: Vec<String> = vec![];
        file_config.map(|c| tags.extend(c.tags().iter().map(|s| s.to_string())));
        author_config.map(|c| tags.extend(c.tags.iter().map(|s| s.to_string())));
        ConfigContext { tags, weight }
    }

    pub fn tags(&self) -> &Vec<String> {
        &self.tags
    }

    pub fn weight(&self) -> f64 {
        self.weight
    }
}
