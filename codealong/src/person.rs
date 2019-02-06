#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub github_login: Option<String>,
}
