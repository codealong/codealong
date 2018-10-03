extern crate hostname;

use serde_json::Value;

use codealong::AnalyzedCommit;

/// Generates a logstash-compatible JSON representation
pub fn serialize(commit: &AnalyzedCommit) -> Value {
    let json = json!({
        "@timestamp": commit.authored_at.to_rfc3339(),
        "@version": "1",
        "message": commit.summary,
        "type": "commit",
        "host": hostname::get_hostname(),
        "author_email": commit.author_email
    });
    json
}
