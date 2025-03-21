# Oocana Rust

![github-actions](https://github.com/oomol/oocana-rust/actions/workflows/build-and-test.yml/badge.svg?branch=main) [![release](https://img.shields.io/github/v/release/oomol/oocana-rust)](https://github.com/oomol/oocana-rust/releases)


[OOMOL studio](https://oomol.com) 的工作流的底层引擎，使用 Rust 实现。

## Install executor

> 为了像在 OOMOL Studio 中一样运行 `JavaScript/TypeScript` block，需要安装 [@oomol/node-executor](https://github.com/oomol/oocana-node)。

```shell
npm install -g @oomol/node-executor @oomol/oocana-sdk
# check nodejs-executor is added to $PATH
which nodejs-executor
```

---

> 为了像 OOMOL Studio 中运行 Python block，我们需要安装 [oocana-python-executor](https://github.com/oomol/oocana-python)。 您可以通过 `pip install python-executor` 安装 `python-executor`。
通常，Python 依赖项会安装在虚拟环境中，并且包管理器会将 `python-executor` 添加到 $PATH（这种行为可能会因包管理器而异）。

```shell
# activate python virtual environment
pip install oocana-python-executor oocana
# check python-executor is added to $PATH
which python-executor
```

## Demo

本项目产物为 cli 可执行程序，支持多种子命令，其中 Run 子命令支持执行 flow 图。

* 安装 mqtt broker 并启动

```bash
# macos: brew install mosquitto
apt update && apt install -y mosquitto 
# 启动 mosquitto 并指定端口 47688。如果你想在前台运行 mosquitto，可以移除 -d 选项。
mosquitto -d -p 47688
```

开发时以 `cargo run` 代替可执行文件，如 `run` 子命令：

```bash
cargo run run examples/base
```

> examples 中有多个示例，你可以尝试。

> 你还可以从 [github release](https://github.com/oomol/oocana-rust/releases) 下载不同平台的 oocana 二进制。

使用说明可以通过 `cargo run help` 查看。

## 构建当前系统架构的二进制

1. 安装 rust。
1. 项目根执行 `cargo build --release`。
1. 产物在 `target/release/oocana`。

* clean

```bash
cargo clean
```

## 日志

发布版本不会将日志打印到 stdout 和 stderr。对于想要在 stdout 和 stderr 中查看日志的用户，可以为 `run` 子命令传递 `--verbose`。

您可以在 `~/.oocana/session/<session_id>/` 中找到所有 `run` 子命令的日志。如果未给出`session id`，oocana 将生成一个新的随机 ID。对于想要指定 ID 的用户，可以为 `run` 子命令传递 `--session-id <session_id>`。

## 项目结构

本项目为 workspace （monorepo）结构。

- `cli`
  配置命令行参数
- `core`
  处理核心调度业务
- `examples`
  一个 Demo 展示如何执行
- `mainframe`
  管理本程序与执行 block 子进程的通信
- `manifest_reader`
  负责从 yaml 文件中读取 flow 图与 block 的 meta 信息并进行处理成内部结构供 core 使用
- `sdk`
  基于 Rust 实现的 Oocana block sdk，供本项目测试使用
- `src`
  Rust 程序入口
- `tests`
  本项目测试
- `utils`
  本项目可复用的一些工具方法
- `npm`
  把不同环境的二进制做成 NPM 包