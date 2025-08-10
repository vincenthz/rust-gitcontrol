use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::{env, process};

mod errors;
mod types;
mod userdb;

use crate::errors::Error;
use crate::types::{Repo, User};
use crate::userdb::{read_db, UserDb};

enum ErrorCode {
    FailedReadingCmdArgs = 1,
    NoHomeEnvironment = 2,
    ExecutingCommandFailed = 3,
    UserNotFound = 4,
    NoSshOriginalCommand = 5,
    PathOfRepositoryInvalid = 6,
    UnknownGitCommand = 7,
    PermissionCheckFailed = 8,
    CannotReadDbFile = 9,
}

impl ErrorCode {
    fn print(&self) -> &'static str {
        match self {
            ErrorCode::FailedReadingCmdArgs => "failed reading command arguments",
            ErrorCode::NoHomeEnvironment => "no HOME environment found",
            ErrorCode::ExecutingCommandFailed => "executing command failed",
            ErrorCode::UserNotFound => "user not found",
            ErrorCode::NoSshOriginalCommand => "no SSH_ORIGINAL_COMMAND found",
            ErrorCode::PathOfRepositoryInvalid => "path of repository invalid",
            ErrorCode::UnknownGitCommand => "unknown git command",
            ErrorCode::PermissionCheckFailed => "Permission insufficient",
            ErrorCode::CannotReadDbFile => "cannot read db file",
        }
    }

    fn exit(self) -> ! {
        process::exit(self as i32)
    }
}

fn fail<A>(e: Result<A, Error>, s: ErrorCode) -> A {
    match e {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {}", s.print(), e);
            s.exit()
        }
    }
}

fn fail_optional<A>(e: Option<A>, s: ErrorCode) -> A {
    match e {
        Some(a) => a,
        None => {
            eprintln!("{}: value not found", s.print());
            s.exit()
        }
    }
}

const GIT_RECEIVE_PACK: &'static str = "git-receive-pack ";
const GIT_UPLOAD_PACK: &'static str = "git-upload-pack ";

#[derive(Debug)]
enum GitCommand {
    GitReceivePack(Repo),
    GitUploadPack(Repo),
}

impl GitCommand {
    pub fn check_permission(&self, db: &UserDb) -> Result<(), Error> {
        match self {
            GitCommand::GitReceivePack(repo) => {
                if !db.can_read(&repo) {
                    Err(Error::AccessDenied("no read permission"))
                } else {
                    Ok(())
                }
            }
            GitCommand::GitUploadPack(repo) => {
                if !db.can_write(&repo) {
                    Err(Error::AccessDenied("no write permission"))
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn execute(&self, home: PathBuf) {
        let mut command = match self {
            GitCommand::GitReceivePack(_) => Command::new("git-receive-pack"),
            GitCommand::GitUploadPack(_) => Command::new("git-upload-pack"),
        };
        match self {
            GitCommand::GitReceivePack(repo) => command.args(&[repo.to_path(&home)]),
            GitCommand::GitUploadPack(repo) => command.args(&[repo.to_path(&home)]),
        };
        let e = command.exec();
        fail::<()>(Err(e.into()), ErrorCode::ExecutingCommandFailed)
    }
}

/*
fn read_git_control(repo: PathBuf) -> process::Output {
    const GIT_DIR_PREFIX : &'static str = "--git-dir=";
    let mut opt = String::new();
    opt.push_str(GIT_DIR_PREFIX);
    opt.push_str(repo.to_str().unwrap());
    Command::new("git").args(&["show", "master:gitcontrol.cfg"])
                       .output()
                       .expect("failed to execute process")
}
*/

fn repository_of_path(s: &str) -> Result<Repo, Error> {
    if s.starts_with("'") && s.ends_with("'") {
        Repo::from_string(s[1..(s.len() - 1)].into())
    } else {
        Repo::from_string(s.into())
    }
}

fn gitcontrol_config_path(home: &PathBuf) -> PathBuf {
    let mut config_path = PathBuf::new();
    config_path.push(home);
    config_path.push("gitcontrol.cfg");
    config_path
}

pub enum Mode {
    Debug(PathBuf, Option<User>),
    Normal(User),
}

fn normal(user: User) {
    let home = fail_optional(env::home_dir(), ErrorCode::NoHomeEnvironment);
    let config_path = gitcontrol_config_path(&home);
    let db = fail(read_db(&config_path, user), ErrorCode::CannotReadDbFile);

    if db.is_empty() {
        eprintln!("user not found (or empty)");
        ErrorCode::UserNotFound.exit()
    }

    let cmd_str = fail(
        env::var("SSH_ORIGINAL_COMMAND").map_err(|e| e.into()),
        ErrorCode::NoSshOriginalCommand,
    );

    let cmd = if cmd_str.starts_with(GIT_RECEIVE_PACK) {
        let s = &cmd_str[GIT_RECEIVE_PACK.len()..];
        let repo = fail(repository_of_path(s), ErrorCode::PathOfRepositoryInvalid);
        GitCommand::GitReceivePack(repo)
    } else if cmd_str.starts_with(GIT_UPLOAD_PACK) {
        let s = &cmd_str[GIT_UPLOAD_PACK.len()..];
        let repo = fail(repository_of_path(s), ErrorCode::PathOfRepositoryInvalid);
        GitCommand::GitUploadPack(repo)
    } else {
        eprintln!("unknown command {}", cmd_str);
        ErrorCode::UnknownGitCommand.exit()
    };
    fail(cmd.check_permission(&db), ErrorCode::PermissionCheckFailed);
    cmd.execute(home)
}

fn debug(config_path: PathBuf, ouser: Option<User>) {
    match ouser {
        None => {
            eprintln!("not implemented")
        }
        Some(user) => {
            let db = fail(read_db(&config_path, user), ErrorCode::CannotReadDbFile);
            for (ref r, ref p) in db.repos {
                println!("{:?} {:?}", r, p)
            }
        }
    }
}

fn parse_argument(args: &Vec<String>) -> Result<Mode, Error> {
    if args.len() < 2 {
        return Err(Error::UsageInvalid("no arguments"));
    }
    if args[1] == "--debug" {
        if args.len() == 2 {
            return Err(Error::UsageInvalid("no arguments"));
        }
        let mut cfg = PathBuf::new();
        cfg.push(args[2].clone());

        if args.len() > 3 {
            let user = User::from_string(args[3].clone())?;
            Ok(Mode::Debug(cfg, Some(user)))
        } else {
            Ok(Mode::Debug(cfg, None))
        }
    } else {
        if args.len() != 2 {
            return Err(Error::UsageInvalid("too many arguments"));
        }
        let user = User::from_string(args[1].clone())?;
        Ok(Mode::Normal(user))
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mode = fail(parse_argument(&args), ErrorCode::FailedReadingCmdArgs);

    match mode {
        Mode::Normal(user) => normal(user),
        Mode::Debug(cfg, ouser) => debug(cfg, ouser),
    }
}
