//! Compile the per-user SSH public keys under a `users/` directory into a
//! single `authorized_keys` file.
//!
//! Layout of the `users/` directory:
//!
//! ```text
//! users/alice        # one file per user, named after the (remote) user
//! users/bob          # each file holds that user's public keys, one per line
//! ```
//!
//! Each public key produces one `authorized_keys` line forcing the
//! gitcontrol shell for that user, as described in the README:
//!
//! ```text
//! command="/usr/bin/gitcontrol-shell alice" ssh-ed25519 AAAA... alice@laptop
//! ```
//!
//! The user name comes from the file name and is validated as a [`User`], and
//! every key line is validated (recognised key type, base64 blob, no control
//! characters) before it is emitted. This matters because the user name and
//! key text are placed on the same line as the forced command: a stray
//! newline or quote would otherwise let an entry escape its `command="..."`
//! restriction.

use std::fs;
use std::path::Path;

use crate::errors::Error;
use crate::types::User;

/// Default path to the shell forced by the generated `command="..."`, matching
/// the README.
pub const DEFAULT_SHELL: &str = "/usr/bin/gitcontrol-shell";

/// SSH public key types accepted in a user's key file.
const KEY_TYPES: &[&str] = &[
    "ssh-ed25519",
    "ssh-rsa",
    "ssh-dss",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "sk-ssh-ed25519@openssh.com",
    "sk-ecdsa-sha2-nistp256@openssh.com",
];

/// Read every user's key file from `users_dir`, returning `(user, contents)`
/// pairs sorted by user name. Hidden files (e.g. `.gitkeep`) are skipped;
/// any other entry whose name is not a valid user name is an error.
pub fn load_user_keys(users_dir: &Path) -> Result<Vec<(User, String)>, Error> {
    let read = fs::read_dir(users_dir).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ConfigInvalid(format!(
                "users directory not found: {}",
                users_dir.display()
            ))
        } else {
            Error::Io(e)
        }
    })?;

    let mut entries: Vec<(User, String)> = Vec::new();
    for entry in read {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_str().ok_or_else(|| {
            Error::ConfigInvalid(format!("non-UTF-8 file name in {}", users_dir.display()))
        })?;

        // Skip hidden helper files such as .gitkeep.
        if name.starts_with('.') {
            continue;
        }

        // Follow symlinks; only regular files hold keys.
        if !fs::metadata(entry.path())?.is_file() {
            continue;
        }

        let user = User::from_string(name.to_string())?;
        let contents = fs::read_to_string(entry.path())?;
        entries.push((user, contents));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

/// Turn `(user, key-file-contents)` pairs into the contents of an
/// `authorized_keys` file. Every non-empty, non-comment line must be a valid
/// public key or an error is returned identifying the offending user and line.
pub fn compile_authorized_keys(entries: &[(User, String)], shell: &str) -> Result<String, Error> {
    if shell.contains('"') || shell.chars().any(|c| c.is_control()) {
        return Err(Error::ConfigInvalid(
            "shell path contains invalid characters".to_string(),
        ));
    }

    let mut out = String::new();
    for (user, contents) in entries {
        for (i, raw) in contents.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Err(msg) = validate_public_key(line) {
                return Err(Error::ConfigInvalid(format!(
                    "user {}: line {}: {}",
                    user,
                    i + 1,
                    msg
                )));
            }
            out.push_str(&format!("command=\"{shell} {user}\" {line}\n"));
        }
    }
    Ok(out)
}

/// Read `users_dir` and compile it into `authorized_keys` contents.
pub fn compile(users_dir: &Path, shell: &str) -> Result<String, Error> {
    let entries = load_user_keys(users_dir)?;
    compile_authorized_keys(&entries, shell)
}

/// Validate that `line` is a plausible single SSH public key: a recognised
/// key type, a base64 blob, and nothing that could break out of the line.
fn validate_public_key(line: &str) -> Result<(), String> {
    if line.chars().any(|c| c.is_control()) {
        return Err("contains control characters".to_string());
    }

    let mut tokens = line.split_whitespace();
    let key_type = tokens.next().ok_or_else(|| "empty key line".to_string())?;
    if !KEY_TYPES.contains(&key_type) {
        return Err(format!("unrecognised key type: {key_type}"));
    }

    let blob = tokens
        .next()
        .ok_or_else(|| "missing key data".to_string())?;
    if blob.is_empty() || !blob.bytes().all(is_base64) {
        return Err("key data is not valid base64".to_string());
    }

    // Any remaining tokens form the (optional) comment; control characters
    // were already rejected, so it cannot contain a newline.
    Ok(())
}

fn is_base64(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='
}

#[cfg(test)]
mod tests {
    use super::*;

    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const RSA: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn user(s: &str) -> User {
        User::from_string(s.to_string()).unwrap()
    }

    #[test]
    fn single_key_gets_forced_command() {
        let entries = vec![(user("alice"), format!("{ED25519} alice@laptop\n"))];
        let out = compile_authorized_keys(&entries, DEFAULT_SHELL).unwrap();
        assert_eq!(
            out,
            format!("command=\"/usr/bin/gitcontrol-shell alice\" {ED25519} alice@laptop\n")
        );
    }

    #[test]
    fn multiple_keys_and_users_each_get_a_line() {
        let entries = vec![
            (user("alice"), format!("{ED25519}\n{RSA}\n")),
            (user("bob"), format!("{ED25519}\n")),
        ];
        let out = compile_authorized_keys(&entries, DEFAULT_SHELL).unwrap();
        assert_eq!(out.lines().count(), 3);
        assert!(
            out.lines()
                .all(|l| l.starts_with("command=\"/usr/bin/gitcontrol-shell "))
        );
        assert_eq!(out.lines().filter(|l| l.contains(" bob\"")).count(), 1);
    }

    #[test]
    fn blank_lines_and_comments_are_skipped() {
        let entries = vec![(
            user("alice"),
            format!("# my keys\n\n{ED25519}\n   \n# another\n{RSA}\n"),
        )];
        let out = compile_authorized_keys(&entries, DEFAULT_SHELL).unwrap();
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn honours_custom_shell() {
        let entries = vec![(user("alice"), format!("{ED25519}\n"))];
        let out = compile_authorized_keys(&entries, "/opt/git/bin/gc-shell").unwrap();
        assert!(out.starts_with("command=\"/opt/git/bin/gc-shell alice\" "));
    }

    #[test]
    fn rejects_unknown_key_type() {
        let entries = vec![(user("alice"), "not-a-key AAAA blah\n".to_string())];
        assert!(compile_authorized_keys(&entries, DEFAULT_SHELL).is_err());
    }

    #[test]
    fn rejects_non_base64_key_data() {
        let entries = vec![(user("alice"), "ssh-ed25519 not_base64!! c\n".to_string())];
        assert!(compile_authorized_keys(&entries, DEFAULT_SHELL).is_err());
    }

    #[test]
    fn rejects_control_characters() {
        // A key line carrying an embedded control character must be rejected so
        // it cannot smuggle a second authorized_keys entry.
        let entries = vec![(user("alice"), format!("{ED25519}\x07evil\n"))];
        assert!(compile_authorized_keys(&entries, DEFAULT_SHELL).is_err());
    }

    #[test]
    fn rejects_shell_with_quote() {
        let entries = vec![(user("alice"), format!("{ED25519}\n"))];
        assert!(compile_authorized_keys(&entries, "/bin/sh\" evil").is_err());
    }

    #[test]
    fn load_user_keys_reads_sorts_and_skips_hidden() {
        let dir =
            std::env::temp_dir().join(format!("gitcontrol-authkeys-test-{}", std::process::id()));
        let users = dir.join("users");
        fs::create_dir_all(&users).unwrap();
        fs::write(users.join("bob"), format!("{ED25519}\n")).unwrap();
        fs::write(users.join("alice"), format!("{RSA}\n")).unwrap();
        fs::write(users.join(".gitkeep"), "").unwrap();

        let entries = load_user_keys(&users).unwrap();
        let names: Vec<&str> = entries.iter().map(|(u, _)| u.as_str()).collect();
        assert_eq!(names, vec!["alice", "bob"]); // sorted, .gitkeep skipped

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_users_dir_is_a_clear_error() {
        let missing = std::env::temp_dir().join("gitcontrol-does-not-exist-xyz");
        let err = load_user_keys(&missing).unwrap_err();
        assert!(matches!(err, Error::ConfigInvalid(_)));
    }
}
