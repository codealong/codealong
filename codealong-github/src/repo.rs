#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub git_url: String,
}
