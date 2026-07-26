use std::env;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    UsageInvalid(&'static str),
    VarError(env::VarError),
    UserInvalid(String),
    RepoInvalid((String, &'static str)),
    PermissionInvalid(char),
    AccessDenied(&'static str),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::UsageInvalid(s) => {
                write!(f, "Usage invalid: {}", s)
            }
            Error::VarError(s) => {
                write!(f, "Var error: {}", s)
            }
            Error::UserInvalid(s) => {
                write!(f, "User Invalid {}", s)
            }
            Error::RepoInvalid((s, t)) => {
                write!(f, "Repo Invalid \"{}\": {}", s, t)
            }
            Error::PermissionInvalid(s) => {
                write!(f, "Permission Invalid {}", s)
            }
            Error::AccessDenied(s) => {
                write!(f, "Access denied {}", s)
            }
            Error::Io(i) => {
                write!(f, "io error {:?}", i)
            }
        }
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}
impl From<env::VarError> for Error {
    fn from(e: env::VarError) -> Error {
        Error::VarError(e)
    }
}
