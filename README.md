# gitcontrol

this is a shell to control access to git repositories over ssh

## Ssh config

on the `authorized_keys` for the local user with the git repository:

```
command="/usr/bin/gitcontrol-shell <remote-user>" <ssh key>
```

Each ssh key `<ssh key>` matching will then be associated as being from the
`<remote-user>` which then used to see what's the permission associated

## Config

the shell looks for a `gitcontrol.cfg` which should contains:

```
@user
w dir/repo
r a/b
w z/a
# comment

@user2
w somedir/repo
@user3

```

## Administration

The `gitcontrol` binary manages the user database and repositories. It reads
and writes the same `gitcontrol.cfg` and creates the bare repositories that
`gitcontrol-shell` serves.

By default it operates on the home directory of the owner user (`git`, looked
up via `getent passwd`); override the location with `--base <dir>` or the
`GITCONTROL_HOME` environment variable. Created files are chowned to `git:git`
(override with `--owner <user:group>`, or skip with `--no-chown`), so the tool
is normally run as root.

```
# create an empty bare repo at <base>/web/site, owned by git:git,
# and grant alice write access to it
gitcontrol repo create alice web/site

# grant read-only access to an existing repo (no repo is created)
gitcontrol repo grant bob web/site -r

# remove an access grant
gitcontrol repo revoke bob web/site

# list every grant, grouped by user
gitcontrol repo list

# manage the user database
gitcontrol user add carol
gitcontrol user show carol
gitcontrol user list
gitcontrol user remove carol
```

Run `gitcontrol --help` for the full list of commands and options.

### SSH keys

Each user's SSH public keys live in a file under `<base>/users/`, named after
the (remote) user, one key per line:

```
users/alice     # ssh-ed25519 AAAA... alice@laptop  (one or more keys)
users/bob
```

`gitcontrol authorized-keys` compiles every key from every user into an
`authorized_keys`, forcing the shell described under [Ssh config](#ssh-config)
on each one:

```
command="/usr/bin/gitcontrol-shell alice" ssh-ed25519 AAAA... alice@laptop
```

```
# regenerate <base>/.ssh/authorized_keys (file mode 0600, .ssh mode 0700)
gitcontrol authorized-keys

# preview without writing anything
gitcontrol authorized-keys --stdout

# write elsewhere, or force a different shell path
gitcontrol authorized-keys --output /home/git/.ssh/authorized_keys
gitcontrol authorized-keys --shell /usr/local/bin/gitcontrol-shell
```

The file name must be a valid user name (letters, digits, `-`, `_`); hidden
files such as `.gitkeep` are ignored. Every key is validated before anything is
written, so a malformed key fails loudly instead of producing a broken
`authorized_keys`.
