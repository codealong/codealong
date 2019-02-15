pub fn build_es_client(matches: &clap::ArgMatches) -> codealong_elk::Client {
    let url = matches
        .value_of("elasticsearch_url")
        .unwrap_or("https://localhost:9200");
    codealong_elk::Client::new(url)
}
