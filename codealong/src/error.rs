use git2;
use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Git2(git2::Error),
    Generic(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Git2(ref e) => fmt::Display::fmt(e, f),
            Error::Generic(s) => f.write_str(s),
        }
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Git2(ref e) => e.description(),
            Error::Generic(s) => s,
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::Git2(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<git2::Error> for Error {
    fn from(error: git2::Error) -> Self {
        Error::Git2(error)
    }
}

impl From<&'static str> for Error {
    fn from(error: &'static str) -> Self {
        Error::Generic(error)
    }
}
