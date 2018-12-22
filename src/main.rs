use std::{env, process};
use std::process::Command;
use std::os::unix::process::CommandExt;
use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::io::BufRead;

mod types;
mod errors;

use crate::errors::Error;
use crate::types::{User, Repo, Permission};


struct UserDb {
    repos: BTreeMap<Repo, Permission>,
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

fn fail<A>(e: Result<A, Error>, s: &'static str) -> A {
    match e {
        Ok(a) => a,
        Err(e) => { println!("{}: {}", s, e); process::exit(1) }
    }
}

fn fail_optional<A>(e: Option<A>, s: &'static str) -> A {
    match e {
        Some(a) => a,
        None => { println!("{}: value not found", s); process::exit(1) }
    }
}

// format:
//
// @user
// w repo
// r repo
// # comment

fn read_db(user: User) -> Result<UserDb, Error> {
    let mut repos = BTreeMap::new();

    let file = File::open("gitcontrol.cfg")?;
    // true if this is the current user
    let mut on_user = false;

    for line in io::BufReader::new(file).lines() {
        let s = line?;
        let mut cs = s.chars();
        match cs.next() {
            None => {},
            Some(c) => {
                if c == '@' {
                    if on_user {
                        on_user = false
                    } else {
                        on_user = user.is_eq(&s[1..]);
                    }
                } else {
                    if !on_user { break; }
                    if c == '#' { break; }

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
const GIT_RECEIVE_PACK : &'static str = "git-receive-pack ";
const GIT_UPLOAD_PACK : &'static str = "git-upload-pack ";

#[derive(Debug)]
enum GitCommand {
    GitReceivePack(Repo),
    GitUploadPack(Repo),
}

impl GitCommand {
    pub fn check_permission(&self, db: &UserDb) -> Result<(), Error> {
        match self {
            GitCommand::GitReceivePack(repo) => {
                if !db.can_read(&repo) { Err(Error::AccessDenied("no read permission")) } else { Ok(()) }
            }
            GitCommand::GitUploadPack(repo) => {
                if !db.can_write(&repo) { Err(Error::AccessDenied("no write permission")) } else { Ok(()) }
            }
        }
    }

    pub fn execute(&self) {
        let home = fail_optional(env::home_dir(), "HOME environment");
        let mut command = match self {
            GitCommand::GitReceivePack(_) => Command::new("git-receive-pack"),
            GitCommand::GitUploadPack(_) => Command::new("git-upload-pack"),
        };
        match self {
            GitCommand::GitReceivePack(repo) => {
                command.args(&[repo.to_path(home)])
            },
            GitCommand::GitUploadPack(repo) => {
                command.args(&[repo.to_path(home)])
            }
        };
        let e = command.exec();
        fail::<()>(Err(e.into()), "executing command")
    }
}

fn repository_of_path(s: &str) -> Result<Repo, Error> {
    if s.starts_with("'") && s.ends_with("'") {
        Repo::from_string(s[1..(s.len()-2)].into())
    } else {
        Repo::from_string(s.into())
    }
}

fn main() {
    let args : Vec<String> = env::args().collect();
    let user = fail(match &args[..] {
        [] => Err(Error::UsageInvalid("no arguments")),
        [_,y] => User::from_string(y.clone()),
        _ => Err(Error::UsageInvalid("too many arguments")),
    }, "reading user argument");
    let db = fail(read_db(user), "cannot read db file");

    if db.is_empty() {
        println!("user not found (or empty)");
        process::exit(1)
    }

    /*
    const print_db : bool = false;
    if print_db {
        for (ref r,ref p) in db.repos {
            println!("{:?} {:?}", r, p)
        }
    }
    */

    let cmd_str = fail(env::var("SSH_ORIGINAL_COMMAND").map_err(|e| e.into()), "getting SSH_ORIGINAL_COMMAND");

    let cmd =
        if cmd_str.starts_with(GIT_RECEIVE_PACK) {
            let s = &cmd_str[GIT_RECEIVE_PACK.len()..];
            let repo = fail(repository_of_path(s), "path of repository invalid");
            GitCommand::GitReceivePack(repo)
        } else if cmd_str.starts_with(GIT_UPLOAD_PACK) {
            let s = &cmd_str[GIT_UPLOAD_PACK.len()..];
            let repo = fail(repository_of_path(s), "path of repository invalid");
            GitCommand::GitUploadPack(repo)
        } else {
            println!("unknown command {}", cmd_str);
            process::exit(1)
        };
    fail(cmd.check_permission(&db), "permission check");
    cmd.execute()
}
