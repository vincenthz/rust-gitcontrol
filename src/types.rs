use super::errors::Error;
use std::fmt;
use std::path::{Path, PathBuf};

fn pattern_not_accepted(c: char) -> bool {
    !(c.is_alphanumeric() || c == '-' || c == '_')
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct User(String);

impl User {
    pub fn from_string(s: String) -> Result<Self, Error> {
        if s.find(pattern_not_accepted).is_some() {
            return Err(Error::UserInvalid(s));
        }
        Ok(User(s))
    }

    pub fn is_eq(&self, s: &str) -> bool {
        self.0 == s
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Repo([String; 2]);

impl Repo {
    pub fn from_string(s: String) -> Result<Self, Error> {
        if !s.is_ascii() {
            return Err(Error::RepoInvalid((s, "repo contains non ASCII")));
        }
        if s.starts_with('/') {
            return Err(Error::RepoInvalid((s, "repo starts with /")));
        }
        if s.starts_with('.') {
            return Err(Error::RepoInvalid((s, "repo starts with .")));
        }

        let ss: Vec<&str> = s.splitn(2, '/').collect();
        if ss.len() < 2 {
            return Err(Error::RepoInvalid((s, "not enough /")));
        } else if ss.len() > 2 {
            return Err(Error::RepoInvalid((s, "more than 2 /")));
        }

        let dir = ss[0];
        let repo = ss[1];

        if dir.find(pattern_not_accepted).is_some() {
            return Err(Error::RepoInvalid((s, "directory is not alphanumeric")));
        }
        if repo.find(pattern_not_accepted).is_some() {
            return Err(Error::RepoInvalid((s, "repo is not alphanumeric")));
        }

        Ok(Repo([dir.to_string(), repo.to_string()]))
    }

    pub fn to_path(&self, prefix: &Path) -> PathBuf {
        [
            prefix.to_path_buf(),
            self.0[0].clone().into(),
            self.0[1].clone().into(),
        ]
        .iter()
        .collect()
    }

    /// the top-level directory component (the part before the `/`)
    pub fn dir(&self) -> &str {
        &self.0[0]
    }

    /// the repository component (the part after the `/`)
    pub fn name(&self) -> &str {
        &self.0[1]
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.0[0], self.0[1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    Read,
    Write,
}

impl Permission {
    pub fn from_char(c: char) -> Result<Self, Error> {
        match c {
            'r' => Ok(Permission::Read),
            'w' => Ok(Permission::Write),
            _ => Err(Error::PermissionInvalid(c)),
        }
    }

    pub fn to_char(self) -> char {
        match self {
            Permission::Read => 'r',
            Permission::Write => 'w',
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Permission::Read => write!(f, "read"),
            Permission::Write => write!(f, "write"),
        }
    }
}
