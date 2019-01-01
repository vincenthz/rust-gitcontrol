use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::io;
use std::io::BufRead;

use crate::errors::Error;
use crate::types::{User, Repo, Permission};

pub struct UserDb {
    pub repos: BTreeMap<Repo, Permission>,
}

impl UserDb {
    pub fn can_read(&self, repo: &Repo) -> bool {
        match self.repos.get(repo) {
            None => false,
            Some(Permission::Read) => true,
            Some(Permission::Write) => true,
        }
    }
    pub fn can_write(&self, repo: &Repo) -> bool {
        match self.repos.get(repo) {
            None => false,
            Some(Permission::Read) => false,
            Some(Permission::Write) => true,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.repos.is_empty()
    }
}

// format:
//
// @user
// w repo
// r repo
// # comment

pub fn read_db(config_path: &PathBuf, user: User) -> Result<UserDb, Error> {
    let mut repos = BTreeMap::new();

    //println!("path: {:?}", config_path);

    let file = File::open(config_path)?;
    // true if this is the current user
    let mut on_user = false;

    for line in io::BufReader::new(file).lines() {
        let s = line?;
        let mut cs = s.chars();
        match cs.next() {
            None => {},
            Some(c) => {
                if c == '@' {
                    on_user = user.is_eq(&s[1..]);
                } else {
                    if !on_user { continue; }
                    if c == '#' { continue; }

                    let permission = Permission::from_char(c)?;

                    let v = cs.next().unwrap();
                    if v != ' ' { panic!("expecting space after permission {:?}", permission) }

                    let repo = cs.collect();
                    repos.insert(Repo::from_string(repo)?, permission);
                }
            },
        }
    }

    Ok(UserDb { repos })
}
