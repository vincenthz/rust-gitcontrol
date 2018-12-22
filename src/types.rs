use super::errors::Error;
use std::path::PathBuf;

fn pattern_not_accepted(c: char) -> bool {
    !c.is_alphanumeric()
}

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct User(String);

impl User {
    pub fn from_string(s: String) -> Result<Self, Error> {
        if s.find(pattern_not_accepted).is_some() {
            return Err(Error::UserInvalid(s))
        }
        Ok(User(s))
    }

    pub fn is_eq(&self, s: &str) -> bool {
        self.0 == s
    }
}

#[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct Repo([String;2]);

impl Repo {
    pub fn from_string(s: String) -> Result<Self, Error> {
        if !s.is_ascii() { return Err(Error::RepoInvalid((s, "repo contains non ASCII"))) }
        if s.starts_with("/") { return Err(Error::RepoInvalid((s, "repo starts with /"))) }
        if s.starts_with(".") { return Err(Error::RepoInvalid((s, "repo starts with ."))) }

        let ss : Vec<&str> = s.splitn(2, "/").collect();
        if ss.len() < 2 {
            return Err(Error::RepoInvalid((s, "not enough /")))
        } else if ss.len() > 2 {
            return Err(Error::RepoInvalid((s, "more than 2 /")))
        }

        let dir = ss[0];
        let repo = ss[1];

        if dir.find(pattern_not_accepted).is_some() {
            return Err(Error::RepoInvalid((s, "directory is not alphanumeric")))
        }
        if repo.find(pattern_not_accepted).is_some() {
            return Err(Error::RepoInvalid((s, "repo is not alphanumeric")))
        }

        Ok(Repo([dir.to_string(), repo.to_string()]))
    }

    pub fn to_path(&self, prefix: PathBuf) -> PathBuf {
        [prefix, self.0[0].clone().into(), self.0[1].clone().into()].iter().collect()
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub enum Permission {
    Read,
    Write
}

impl Permission {
    pub fn from_char(c: char) -> Result<Self, Error> {
        match c {
            'r' => Ok(Permission::Read),
            'w' => Ok(Permission::Write),
            _   => Err(Error::PermissionInvalid(c))
        }
    }
}
