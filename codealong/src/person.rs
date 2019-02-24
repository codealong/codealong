#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub github_login: Option<String>,
    pub teams: Vec<String>,
}
