use crate::identity::Identity;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    #[serde(default)]
    pub identities: Vec<Identity>,
    #[serde(default)]
    pub github_logins: Vec<String>,
    #[serde(default)]
    pub teams: Vec<String>,
}

impl Person {
    pub fn from_identity(identity: &Identity) -> Person {
        Person {
            id: identity.to_string(),
            identities: vec![identity.to_owned()],
            ..Default::default()
        }
    }

    pub fn from_github_login(github_login: &str) -> Person {
        Person {
            id: github_login.to_owned(),
            github_logins: vec![github_login.to_owned()],
            ..Default::default()
        }
    }

    pub fn partial(&self) -> PartialPerson {
        PartialPerson {
            id: self.id.clone(),
            name: self.identities.first().and_then(|id| id.name.clone()),
            email: self.identities.first().and_then(|id| id.email.clone()),
            github_login: self.github_logins.first().cloned(),
            teams: self.teams.clone(),
        }
    }
}

impl Default for Person {
    fn default() -> Self {
        Self {
            id: "".to_owned(),
            identities: Vec::new(),
            github_logins: Vec::new(),
            teams: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialPerson {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub github_login: Option<String>,
    pub teams: Vec<String>,
}
