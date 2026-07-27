//! `gitcontrol` — administer the gitcontrol user database and repositories.
//!
//! Creating a repository does three things: it initialises an empty bare git
//! repository under the base directory, grants the requested user access to it
//! in `gitcontrol.cfg`, and sets ownership of the created files to the owner
//! user/group (`git:git` by default) so the `gitcontrol-shell` can serve them.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};

use gitcontrol_shell::authkeys;
use gitcontrol_shell::config::Config;
use gitcontrol_shell::errors::Error;
use gitcontrol_shell::types::{Permission, Repo, User};

const USAGE: &str = "\
gitcontrol - administer the gitcontrol user database and repositories

Usage:
    gitcontrol [options] <command>

Options:
    --base <dir>          base directory holding the repositories and
                          gitcontrol.cfg. Defaults to the home directory of the
                          owner user (looked up via `getent passwd`) or the
                          GITCONTROL_HOME environment variable.
    --owner <user:group>  ownership applied to created files (default: git:git)
    --no-chown            do not change ownership of created files
    -h, --help            show this help

Commands:
    repo create <user> <dir/repo> [-r|-w]   create a bare repo and grant access
    repo grant  <user> <dir/repo> [-r|-w]   grant/update access (no repo created)
    repo revoke <user> <dir/repo>           remove access to a repo
    repo list                               list every grant, grouped by user

    user add    <user>                      add an empty user entry
    user remove <user>                      remove a user and all their grants
    user list                               list users
    user show   <user>                      show a single user's grants

    authorized-keys [--stdout] [--output <path>] [--shell <path>]
                                            compile <base>/users/* into an
                                            authorized_keys file

Permission flags default to write (-w); pass -r for read-only.

The `authorized-keys` command reads one file per user from <base>/users/ (the
file name is the user name, each file holds that user's public keys, one per
line) and writes an authorized_keys forcing `command=\"<shell> <user>\"` on
every key. It defaults to writing <base>/.ssh/authorized_keys (mode 0600, .ssh
mode 0700); use --stdout to print instead, --output to choose another file, and
--shell to override the forced shell path (default /usr/bin/gitcontrol-shell).
";

struct Options {
    base: Option<PathBuf>,
    owner: String,
    chown: bool,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut opts = Options {
        base: None,
        owner: "git:git".to_string(),
        chown: true,
    };

    // Global options must precede the command.
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => {
                i += 1;
                let v = args
                    .get(i)
                    .unwrap_or_else(|| usage_exit("--base requires a directory"));
                opts.base = Some(PathBuf::from(v));
            }
            "--owner" => {
                i += 1;
                let v = args
                    .get(i)
                    .unwrap_or_else(|| usage_exit("--owner requires a user:group"));
                opts.owner = v.clone();
            }
            "--no-chown" => opts.chown = false,
            "-h" | "--help" | "help" => {
                print!("{USAGE}");
                return;
            }
            _ => break,
        }
        i += 1;
    }

    if let Err(e) = dispatch(&opts, &args[i..]) {
        eprintln!("gitcontrol: {e}");
        exit(1);
    }
}

fn dispatch(opts: &Options, args: &[String]) -> Result<(), Error> {
    let (cmd, rest) = split_or_usage(args, "missing command (try `gitcontrol --help`)");
    match cmd {
        "repo" => repo_cmd(opts, rest),
        "user" => user_cmd(opts, rest),
        "authorized-keys" | "keys" => keys_cmd(opts, rest),
        other => usage_exit(&format!("unknown command: {other}")),
    }
}

fn repo_cmd(opts: &Options, args: &[String]) -> Result<(), Error> {
    let (sub, rest) = split_or_usage(args, "missing repo subcommand (create|grant|revoke|list)");
    match sub {
        "create" => {
            let (user, repo, perm) = parse_user_repo_perm(rest)?;
            repo_create(opts, &user, &repo, perm)
        }
        "grant" => {
            let (user, repo, perm) = parse_user_repo_perm(rest)?;
            grant(opts, &user, &repo, perm)
        }
        "revoke" => {
            let (user, repo) = parse_user_repo(rest)?;
            revoke(opts, &user, &repo)
        }
        "list" => repo_list(opts),
        other => usage_exit(&format!("unknown repo subcommand: {other}")),
    }
}

fn user_cmd(opts: &Options, args: &[String]) -> Result<(), Error> {
    let (sub, rest) = split_or_usage(args, "missing user subcommand (add|remove|list|show)");
    match sub {
        "add" => user_add(opts, &parse_one_user(rest)?),
        "remove" | "del" => user_remove(opts, &parse_one_user(rest)?),
        "list" => user_list(opts),
        "show" => user_show(opts, &parse_one_user(rest)?),
        other => usage_exit(&format!("unknown user subcommand: {other}")),
    }
}

// --- commands -------------------------------------------------------------

fn repo_create(opts: &Options, user: &User, repo: &Repo, perm: Permission) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    let repo_path = repo.to_path(&base);
    if repo_path.exists() {
        return Err(Error::RepoExists(repo.to_string()));
    }

    // Ensure the containing directory (base/<dir>) exists.
    if let Some(parent) = repo_path.parent() {
        fs::create_dir_all(parent)?;
    }

    run_command(
        "git",
        &[
            OsStr::new("init"),
            OsStr::new("--bare"),
            repo_path.as_os_str(),
        ],
    )?;

    if opts.chown {
        chown(opts, true, &repo_path)?;
    }

    edit_config(opts, &base, |cfg| {
        cfg.set_permission(user, repo.clone(), perm);
        Ok(())
    })?;

    println!("created {repo} and granted {user} {perm} access");
    Ok(())
}

fn grant(opts: &Options, user: &User, repo: &Repo, perm: Permission) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    edit_config(opts, &base, |cfg| {
        cfg.set_permission(user, repo.clone(), perm);
        Ok(())
    })?;
    println!("granted {user} {perm} access to {repo}");
    Ok(())
}

fn revoke(opts: &Options, user: &User, repo: &Repo) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    let mut removed = false;
    edit_config(opts, &base, |cfg| {
        removed = cfg.revoke(user, repo);
        Ok(())
    })?;
    if removed {
        println!("revoked {user} access to {repo}");
    } else {
        println!("{user} had no access to {repo}");
    }
    Ok(())
}

fn repo_list(opts: &Options) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    let cfg = Config::load(&config_path(&base))?;
    for user in cfg.users() {
        println!("@{user}");
        if let Some(repos) = cfg.permissions_of(user) {
            for (repo, perm) in repos {
                println!("  {} {repo}", perm.to_char());
            }
        }
    }
    Ok(())
}

fn user_add(opts: &Options, user: &User) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    edit_config(opts, &base, |cfg| cfg.add_user(user.clone()))?;
    println!("added user {user}");
    Ok(())
}

fn user_remove(opts: &Options, user: &User) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    edit_config(opts, &base, |cfg| {
        if cfg.remove_user(user) {
            Ok(())
        } else {
            Err(Error::UserUnknown(user.as_str().to_string()))
        }
    })?;
    println!("removed user {user}");
    Ok(())
}

fn user_list(opts: &Options) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    let cfg = Config::load(&config_path(&base))?;
    for user in cfg.users() {
        println!("{user}");
    }
    Ok(())
}

fn user_show(opts: &Options, user: &User) -> Result<(), Error> {
    let base = resolve_base(opts)?;
    let cfg = Config::load(&config_path(&base))?;
    match cfg.permissions_of(user) {
        None => Err(Error::UserUnknown(user.as_str().to_string())),
        Some(repos) => {
            for (repo, perm) in repos {
                println!("{} {repo}", perm.to_char());
            }
            Ok(())
        }
    }
}

fn keys_cmd(opts: &Options, args: &[String]) -> Result<(), Error> {
    let mut to_stdout = false;
    let mut output: Option<PathBuf> = None;
    let mut shell = authkeys::DEFAULT_SHELL.to_string();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--stdout" => to_stdout = true,
            "--output" => {
                i += 1;
                output = Some(PathBuf::from(
                    args.get(i)
                        .unwrap_or_else(|| usage_exit("--output requires a path")),
                ));
            }
            "--shell" => {
                i += 1;
                shell = args
                    .get(i)
                    .unwrap_or_else(|| usage_exit("--shell requires a path"))
                    .clone();
            }
            other => usage_exit(&format!("unexpected argument: {other}")),
        }
        i += 1;
    }

    let base = resolve_base(opts)?;
    let content = authkeys::compile(&base.join("users"), &shell)?;
    let keys = content.lines().count();

    if to_stdout {
        print!("{content}");
        return Ok(());
    }

    let default_path = output.is_none();
    let path = output.unwrap_or_else(|| base.join(".ssh").join("authorized_keys"));
    write_authorized_keys(opts, &path, &content, default_path)?;

    if keys == 0 {
        eprintln!(
            "gitcontrol: warning: no keys found in {}/users",
            base.display()
        );
    }
    println!("wrote {keys} key(s) to {}", path.display());
    Ok(())
}

// --- helpers --------------------------------------------------------------

/// Write `content` to an authorized_keys file atomically with the modes sshd
/// expects (0600 on the file; 0700 on the enclosing `.ssh` when we own the
/// default location). Ownership is fixed to the owner user unless disabled.
fn write_authorized_keys(
    opts: &Options,
    path: &Path,
    content: &str,
    default_path: bool,
) -> Result<(), Error> {
    let parent = path.parent();
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
        if default_path {
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }

    let mut tmp_name = path
        .file_name()
        .ok_or_else(|| Error::ConfigInvalid("output path has no file name".to_string()))?
        .to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);

    fs::write(&tmp, content)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    fs::rename(&tmp, path)?;

    if opts.chown {
        if default_path && let Some(parent) = parent {
            chown(opts, false, parent)?;
        }
        chown(opts, false, path)?;
    }
    Ok(())
}

/// Load the config, apply `f`, and write it back (and, if enabled, fix its
/// ownership). If `f` fails nothing is written.
fn edit_config<F>(opts: &Options, base: &Path, f: F) -> Result<(), Error>
where
    F: FnOnce(&mut Config) -> Result<(), Error>,
{
    let cfg_path = config_path(base);
    let mut cfg = Config::load(&cfg_path)?;
    f(&mut cfg)?;
    cfg.save(&cfg_path)?;
    if opts.chown {
        chown(opts, false, &cfg_path)?;
    }
    Ok(())
}

fn config_path(base: &Path) -> PathBuf {
    base.join("gitcontrol.cfg")
}

fn resolve_base(opts: &Options) -> Result<PathBuf, Error> {
    if let Some(base) = &opts.base {
        return Ok(base.clone());
    }
    if let Ok(home) = env::var("GITCONTROL_HOME")
        && !home.is_empty()
    {
        return Ok(PathBuf::from(home));
    }
    let owner_user = opts.owner.split(':').next().unwrap_or("git");
    if let Some(home) = home_dir_of(owner_user) {
        return Ok(home);
    }
    Err(Error::ConfigInvalid(format!(
        "could not determine the base directory for user '{owner_user}'; \
         pass --base <dir> or set GITCONTROL_HOME"
    )))
}

/// Look up a user's home directory from the passwd database via `getent`.
fn home_dir_of(user: &str) -> Option<PathBuf> {
    let out = Command::new("getent")
        .args(["passwd", user])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    // name:passwd:uid:gid:gecos:home:shell
    let home = text.trim_end().split(':').nth(5)?;
    if home.is_empty() {
        None
    } else {
        Some(PathBuf::from(home))
    }
}

fn chown(opts: &Options, recursive: bool, path: &Path) -> Result<(), Error> {
    let mut args: Vec<&OsStr> = Vec::new();
    if recursive {
        args.push(OsStr::new("-R"));
    }
    args.push(OsStr::new(&opts.owner));
    args.push(path.as_os_str());
    run_command("chown", &args)
}

fn run_command(program: &str, args: &[&OsStr]) -> Result<(), Error> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| Error::CommandFailed(format!("{program}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailed(format!(
            "{program} exited with {status}"
        )))
    }
}

// --- argument parsing -----------------------------------------------------

fn split_or_usage<'a>(args: &'a [String], msg: &str) -> (&'a str, &'a [String]) {
    match args.split_first() {
        Some((first, rest)) => (first.as_str(), rest),
        None => usage_exit(msg),
    }
}

fn parse_user_repo_perm(args: &[String]) -> Result<(User, Repo, Permission), Error> {
    let mut positional: Vec<&str> = Vec::new();
    let mut perm = Permission::Write;
    for a in args {
        match a.as_str() {
            "-r" | "--read" => perm = Permission::Read,
            "-w" | "--write" => perm = Permission::Write,
            s if s.starts_with('-') => usage_exit(&format!("unknown flag: {s}")),
            s => positional.push(s),
        }
    }
    if positional.len() != 2 {
        usage_exit("expected <user> <dir/repo>");
    }
    let user = User::from_string(positional[0].to_string())?;
    let repo = Repo::from_string(positional[1].to_string())?;
    Ok((user, repo, perm))
}

fn parse_user_repo(args: &[String]) -> Result<(User, Repo), Error> {
    if args.len() != 2 {
        usage_exit("expected <user> <dir/repo>");
    }
    let user = User::from_string(args[0].clone())?;
    let repo = Repo::from_string(args[1].clone())?;
    Ok((user, repo))
}

fn parse_one_user(args: &[String]) -> Result<User, Error> {
    if args.len() != 1 {
        usage_exit("expected a single <user>");
    }
    User::from_string(args[0].clone())
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("gitcontrol: {msg}\n");
    eprint!("{USAGE}");
    exit(2);
}
