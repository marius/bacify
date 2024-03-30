# Bacify

## Description

The name is short for Backup&Verify.

Bacify looks for files that
   * should be in the backup (according to the source file birth time) but are not
   * and files that have the same modification timestamp as in the backup but have different content.

## Usage

Only the fantastic [restic](https://github.com/restic/restic) is supported at the moment!

Set the `RESTIC_REPOSITORY` and `RESTIC_PASSWORD` environment variables and run `cargo run`.

### Example (assuming you cloned Bacify into *$HOME/dev/bacify*):

Create backup and verify the data in the repository:
```
$ export RESTIC_REPOSITORY="$HOME/tmp/restic-repo"
$ export RESTIC_PASSWORD="foo"
$ restic init
$ restic backup $HOME/dev/bacify
$ restic check --read-data
```

Verify the backup against the local files:
```
$ cd ~/dev/bacify
$ export RESTIC_REPOSITORY="$HOME/tmp/restic-repo"
$ export RESTIC_PASSWORD="foo"
$ export LOG_LEVEL=debug
$ cargo run
```

> [!WARNING]
> Read this is you get a lot of errors about missing files!<br>
> At the moment there is only support for a hard-coded, single exclude file named `$HOME/.backup_exclude`.<br>
> Bacify does ***NOT*** (yet) support the full exclude file syntax, only prefixes are compared!

## License

MIT