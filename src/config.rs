//! Read/modify/write model for the `gitcontrol.cfg` user database.
//!
//! The on-disk format is line based:
//!
//! ```text
//! @user
//! w dir/repo
//! r dir/repo
//! # comment
//! ```
//!
//! Unlike [`crate::userdb::read_db`], which reads permissions for a single
//! user at request time, [`Config`] holds the whole database so the admin
//! tool can add/remove users and grant/revoke permissions and write it back.
//!
//! Rewriting is canonical: user sections are kept in their original order,
//! each user's repositories are sorted, and comments are not preserved.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::errors::Error;
use crate::types::{Permission, Repo, User};

pub struct Config {
    /// User sections in file order. A user may have no repositories.
    entries: Vec<(User, BTreeMap<Repo, Permission>)>,
}

impl Config {
    pub fn parse(contents: &str) -> Result<Config, Error> {
        let mut entries: Vec<(User, BTreeMap<Repo, Permission>)> = Vec::new();
        let mut current: Option<usize> = None;

        for (i, raw) in contents.lines().enumerate() {
            let lineno = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(name) = line.strip_prefix('@') {
                let user = User::from_string(name.trim().to_string())?;
                current = Some(match entries.iter().position(|(u, _)| u == &user) {
                    Some(idx) => idx,
                    None => {
                        entries.push((user, BTreeMap::new()));
                        entries.len() - 1
                    }
                });
                continue;
            }

            let idx = current.ok_or_else(|| {
                Error::ConfigInvalid(format!("line {lineno}: permission before any @user"))
            })?;

            let mut chars = line.chars();
            let permission = Permission::from_char(chars.next().unwrap())?;
            let rest = chars.as_str().strip_prefix(' ').ok_or_else(|| {
                Error::ConfigInvalid(format!("line {lineno}: expected space after permission"))
            })?;
            let repo = Repo::from_string(rest.trim().to_string())?;
            entries[idx].1.insert(repo, permission);
        }

        Ok(Config { entries })
    }

    /// Load the database from `path`. A missing file yields an empty database.
    pub fn load(path: &Path) -> Result<Config, Error> {
        match fs::read_to_string(path) {
            Ok(s) => Config::parse(&s),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config {
                entries: Vec::new(),
            }),
            Err(e) => Err(e.into()),
        }
    }

    /// Write the database back to `path` atomically (write to a sibling temp
    /// file, then rename over the target).
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let name = path
            .file_name()
            .ok_or_else(|| Error::ConfigInvalid("config path has no file name".to_string()))?;
        let mut tmp_name = name.to_os_string();
        tmp_name.push(".tmp");
        let tmp = path.with_file_name(tmp_name);

        fs::write(&tmp, self.to_string())?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn has_user(&self, user: &User) -> bool {
        self.entries.iter().any(|(u, _)| u == user)
    }

    pub fn add_user(&mut self, user: User) -> Result<(), Error> {
        if self.has_user(&user) {
            return Err(Error::UserExists(user.as_str().to_string()));
        }
        self.entries.push((user, BTreeMap::new()));
        Ok(())
    }

    /// Remove a user and all their permissions. Returns `false` if unknown.
    pub fn remove_user(&mut self, user: &User) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(u, _)| u != user);
        self.entries.len() != before
    }

    /// Grant (or update) a permission on a repository for a user, creating the
    /// user section if it does not exist yet.
    pub fn set_permission(&mut self, user: &User, repo: Repo, permission: Permission) {
        match self.entries.iter_mut().find(|(u, _)| u == user) {
            Some((_, repos)) => {
                repos.insert(repo, permission);
            }
            None => {
                let mut repos = BTreeMap::new();
                repos.insert(repo, permission);
                self.entries.push((user.clone(), repos));
            }
        }
    }

    /// Revoke a permission. Returns `false` if the user had no such grant.
    pub fn revoke(&mut self, user: &User, repo: &Repo) -> bool {
        match self.entries.iter_mut().find(|(u, _)| u == user) {
            Some((_, repos)) => repos.remove(repo).is_some(),
            None => false,
        }
    }

    /// All user sections in file order.
    pub fn users(&self) -> impl Iterator<Item = &User> {
        self.entries.iter().map(|(u, _)| u)
    }

    /// A single user's repositories, or `None` if the user is unknown.
    pub fn permissions_of(&self, user: &User) -> Option<&BTreeMap<Repo, Permission>> {
        self.entries
            .iter()
            .find(|(u, _)| u == user)
            .map(|(_, repos)| repos)
    }

    /// Every `(user, repo, permission)` triple across the database.
    pub fn grants(&self) -> impl Iterator<Item = (&User, &Repo, Permission)> {
        self.entries
            .iter()
            .flat_map(|(u, repos)| repos.iter().map(move |(r, p)| (u, r, *p)))
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, (user, repos)) in self.entries.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            writeln!(f, "@{user}")?;
            for (repo, permission) in repos {
                writeln!(f, "{} {repo}", permission.to_char())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(s: &str) -> User {
        User::from_string(s.to_string()).unwrap()
    }

    fn repo(s: &str) -> Repo {
        Repo::from_string(s.to_string()).unwrap()
    }

    #[test]
    fn parses_users_and_permissions() {
        let cfg = Config::parse("@alice\nw dir/repo\nr a/b\n# comment\n\n@bob\n").unwrap();
        assert_eq!(cfg.users().count(), 2);
        assert!(cfg.has_user(&user("alice")));
        assert!(cfg.has_user(&user("bob")));
        assert_eq!(
            cfg.permissions_of(&user("alice"))
                .unwrap()
                .get(&repo("dir/repo")),
            Some(&Permission::Write)
        );
        assert_eq!(
            cfg.permissions_of(&user("alice"))
                .unwrap()
                .get(&repo("a/b")),
            Some(&Permission::Read)
        );
        // bob exists but has no repositories
        assert!(cfg.permissions_of(&user("bob")).unwrap().is_empty());
    }

    #[test]
    fn round_trips_and_sorts_repos() {
        let cfg = Config::parse("@alice\nw z/z\nr a/a\n").unwrap();
        // repositories are emitted in sorted order
        assert_eq!(cfg.to_string(), "@alice\nr a/a\nw z/z\n");
    }

    #[test]
    fn set_and_revoke_permission() {
        let mut cfg = Config::parse("@alice\n").unwrap();
        cfg.set_permission(&user("alice"), repo("x/y"), Permission::Read);
        assert!(
            cfg.grants()
                .any(|(u, r, p)| u.is_eq("alice") && r == &repo("x/y") && p == Permission::Read)
        );

        // updating changes the permission in place
        cfg.set_permission(&user("alice"), repo("x/y"), Permission::Write);
        assert_eq!(
            cfg.permissions_of(&user("alice"))
                .unwrap()
                .get(&repo("x/y")),
            Some(&Permission::Write)
        );

        assert!(cfg.revoke(&user("alice"), &repo("x/y")));
        assert!(!cfg.revoke(&user("alice"), &repo("x/y")));
    }

    #[test]
    fn set_permission_creates_missing_user() {
        let mut cfg = Config::parse("").unwrap();
        cfg.set_permission(&user("carol"), repo("d/r"), Permission::Write);
        assert!(cfg.has_user(&user("carol")));
    }

    #[test]
    fn add_duplicate_user_is_error() {
        let mut cfg = Config::parse("@alice\n").unwrap();
        assert!(cfg.add_user(user("alice")).is_err());
        assert!(cfg.add_user(user("dave")).is_ok());
    }

    #[test]
    fn remove_user() {
        let mut cfg = Config::parse("@alice\nw d/r\n@bob\n").unwrap();
        assert!(cfg.remove_user(&user("alice")));
        assert!(!cfg.has_user(&user("alice")));
        assert!(!cfg.remove_user(&user("alice")));
    }

    #[test]
    fn rejects_permission_before_user() {
        assert!(Config::parse("w d/r\n").is_err());
    }
}
