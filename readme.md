# WatchRS - A hot-reload application of Rust Development

## What is WatchRS?

WatchRS is a simple application that can be installed via Cargo to implement a "nodemon" style hot-reload system for Rust development.

WatchRS is ran in the current directory in which it has been called, it checks for any file changes recursively below the parent directory. On finding a newly saved file, it will kill the current process and restart the applciation. This is best used for UI development as it will reopen the UI to display new changes.

By default, WatchRS runs the `cargo run` command in the current directory - and ignores the `target` directory.

Install using:

```bash
    cargo install watchrs@0.1.0
```

Run using (in the desired directory):

```bash
    watchrs
```

## TODO:

- [x] Windows Support.
- [ ] Linux Support.
- [ ] MacOS Support.
- [ ] Add argument for customisable target directories.
- [x] Add argument for ignored directories.
- [x] Add argument for target filetype.
- [ ] Accept object (of unspecified type) to gather arguments/features from (nodemon style).
- [ ] Run check for duplicate spawning of resources.
- [ ] Incorporate a better way of monitoring running instances (processes).
- [ ] Write TOML configuration reader.
- [ ] Make compatible as nodemon replacement?
