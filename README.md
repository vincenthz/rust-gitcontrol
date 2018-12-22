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
