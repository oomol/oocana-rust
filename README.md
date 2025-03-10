# Oocana Rust

![github-actions](https://github.com/oomol/oocana-rust/actions/workflows/build-and-test.yml/badge.svg?branch=main) [![release](https://img.shields.io/github/v/release/oomol/oocana-rust)](https://github.com/oomol/oocana-rust/releases) [![readme_zh-cn](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-none)](README.zh-CN.md)

[OOMOL studio](https://oomol.com)'s workflow engine, implemented in Rust.

To support run `javascript`/`typescript` block as OOMOL studio, we need to use [node-executor](https://github.com/oomol/oocana-node). Add `nodejs-executor`(build in oocana-node) to `$PATH` to support run `javascript`/`typescript` block.

To support run `python` block as OOMOL studio, we need to use [python-executor](https://github.com/oomol/oocana-python). you can install `python-executor` by `pip install python-executor`. Typically python dependencies are installed in a virtual environment and the package manager will add `python-executor` to `$PATH` (this behavior may vary depending on the package manager).

## How to Use

This project produces a CLI executable program that supports multiple subcommands, among which the `run` subcommand supports executing flow.


* install a mqtt broker and start it

```bash
# for macos: brew install mosquitto
apt update && apt install -y mosquitto
# start mosquitto with port 47688 in background. If you want run mosquitto in foreground, you can remove -d option.
mosquitto -d -p 47688
```

* run oocana command

During development, use `cargo run` instead of the executable file, such as the `run` subcommand:
```bash
cargo run run examples/base
```

> examples has multiple examples, you can try them.


Usage instructions can be viewed through `cargo run help`.

## Build Binary for Current System Architecture

1. Install Rust.
2. Execute `cargo build --release` at the project root.
3. The output will be at `target/release/oocana`.

clean:
```bash
cargo clean
```

## Log

release version won't print log to stdout and stderr. For user who want to see log in stdout and stderr, you can need pass `--verbose` for `run` subcommand.
You can find all `run` subcommand logs in `~/.oocana/session/<session_id>/`. Session id need to be replaced with the actual session id, if not given, oocana will generate a new random session id. For user who want to specify session id, you can pass `--session <session_id>` for `run` subcommand.

## Project Structure

This project uses a workspace (monorepo) structure.

- `cli`
  Configures command line parameters
- `core`
  Handles core scheduling business
- `examples`
  A demo showing how to execute
- `mainframe`
  Manages communication between this program and executed block subprocesses
- `manifest_reader`
  Responsible for reading flow diagrams and block meta information from YAML files and processing them into internal structures for core to use
- `sdk`
  Oocana block SDK implemented in Rust, used for testing this project
- `src`
  Rust program entry point
- `tests`
  Project tests
- `utils`
  Reusable utility methods for this project
- `npm`
  Packages binaries for different environments as NPM packages