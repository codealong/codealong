use regex::Regex;
use std::fmt;

/// Simple wrapper for Name <Email> strings
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

impl Identity {
    pub fn to_string(&self) -> String {
        format!("{}", self)
    }

    pub fn parse(s: &str) -> Self {
        lazy_static! {
            static ref NAME_EMAIL_REGEX: Regex =
                Regex::new(r#"(?P<name>[^<]*[^< ])? *(?:<(?P<email>.*)>)?"#).unwrap();
        }

        if let Some(captures) = NAME_EMAIL_REGEX.captures(s) {
            Identity {
                name: captures.name("name").map(|m| m.as_str().to_owned()),
                email: captures.name("email").map(|m| m.as_str().to_owned()),
            }
        } else {
            Identity::default()
        }
    }

    pub fn only_name(&self) -> Option<Identity> {
        if None == self.name {
            return None;
        };
        Some(Identity {
            name: self.name.clone(),
            email: None,
        })
    }

    pub fn only_email(&self) -> Option<Identity> {
        if None == self.email {
            return None;
        };
        Some(Identity {
            name: None,
            email: self.email.clone(),
        })
    }

    pub fn partial_eq(&self, other: &Identity) -> bool {
        if self.email.is_none() || other.email.is_none() {
            // if either identity has no email, we just compare names
            self.name == other.name
        } else {
            // otherwise the emails must be the same
            self.email == other.email
        }
    }
}

impl Default for Identity {
    fn default() -> Self {
        Identity {
            name: None,
            email: None,
        }
    }
}

impl<'a> From<git2::Signature<'a>> for Identity {
    fn from(sig: git2::Signature<'a>) -> Self {
        Identity {
            name: sig.name().map(|s| s.to_owned()),
            email: sig.email().map(|s| s.to_owned()),
        }
    }
}

impl<'a> fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.name.is_some() && self.email.is_some() {
            write!(
                f,
                "{} <{}>",
                self.name.as_ref().unwrap(),
                self.email.as_ref().unwrap()
            )
        } else if self.name.is_some() {
            write!(f, "{}", self.name.as_ref().unwrap())
        } else if self.email.is_some() {
            write!(f, "<{}>", self.email.as_ref().unwrap())
        } else {
            write!(f, "")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(
            Identity::parse("<test@test.com>"),
            Identity {
                name: None,
                email: Some("test@test.com".to_owned())
            }
        );

        assert_eq!(
            Identity::parse("Test User <test@test.com>"),
            Identity {
                name: Some("Test User".to_owned()),
                email: Some("test@test.com".to_owned())
            }
        );

        assert_eq!(
            Identity::parse("Test User"),
            Identity {
                name: Some("Test User".to_owned()),
                email: None
            }
        );
    }
}
