#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
}
