# Bacify

[![Rust](https://github.com/marius/bacify/actions/workflows/rust.yml/badge.svg)](https://github.com/marius/bacify/actions/workflows/rust.yml)

## Description

The name is short for Backup&Verify.

Bacify looks for files that
   * should be in the backup (according to the source file birth time) but are not
   * and files that have the same modification timestamp as in the backup but have different content.

## Usage

Only the fantastic [restic](https://github.com/restic/restic) is supported at the moment!
(And only backup snapshots of absolute paths.)

Set the `RESTIC_REPOSITORY` and `RESTIC_PASSWORD` environment variables and run `cargo run`.

### Examples

NOTE: Assuming you cloned Bacify into *$HOME/dev/bacify*

#### Backup snapshot with an absolute path

Create backup and verify the data in the repository:
```
$ cd $HOME/dev/bacify
$ export RESTIC_REPOSITORY="$HOME/tmp/restic-repo"
$ export RESTIC_PASSWORD="foo"
$ restic init
$ restic backup $HOME/dev/bacify
```

Verify the backup against the local files:
```
$ cargo run
```

#### Backup snapshot with a relative path

Create backup and verify the data in the repository:
```
$ cd $HOME/dev/bacify
$ export RESTIC_REPOSITORY="$HOME/tmp/restic-repo"
$ export RESTIC_PASSWORD="foo"
$ restic init
$ restic backup .
```

Verify the backup against the local files:
```
$ cargo run -- --relative-path
```

*--relative-path* is needed as the snapshot metadata lists absolute paths,
but the files are actually restored without the leading path components.

### Excludes

> [!WARNING]
> Read this is you get a lot of errors about missing files!<br>
> At the moment there is only support for a hard-coded, single exclude file named `$HOME/.backup_exclude`.<br>
> Bacify does ***NOT*** (yet) support the full exclude file syntax, only prefixes are compared!

## License

MIT