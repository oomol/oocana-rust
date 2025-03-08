# Oocana Rust

![github-actions](https://github.com/oomol/oocana-rust/actions/workflows/build-and-test.yml/badge.svg?branch=main)

## Demo

本项目产物为 cli 可执行程序，支持多种子命令。

开发时以 `cargo run` 代替可执行文件，如 `run` 子命令：

```bash
cargo run run examples
```

使用说明可以通过 `cargo run help` 查看。

## 构建当前系统架构的二进制

1. 安装 rust。
1. 项目根执行 `cargo build --release`。
1. 产物在 `target/release/oocana`。

clean:
```bash
  cargo clean
  find . -type f -name "*.orig" -exec rm {} \;
  find . -type f -name "*.bk" -exec rm {} \;
  find . -type f -name ".*~" -exec rm {} \;
```

## 打 NPM 包发布多平台二进制

- 第一次需要安装其它平台的工具链（@TODO 增加脚本自动安装）
  ```bash
  rustup target add x86_64-apple-darwin
  rustup target add aarch64-apple-darwin
  rustup target add x86_64-unknown-linux-gnu
  rustup target add aarch64-unknown-linux-gnu
  
  brew tap messense/macos-cross-toolchains
  brew install x86_64-unknown-linux-gnu
  brew install aarch64-unknown-linux-gnu
  ```
- 先更新 `Cargo.toml` 中的版本。
- Macos 用户在当前项目的父一级目录，或者在 `~/.cargo/config` 中增加如下配置：
    ```toml
    [target.x86_64-unknown-linux-gnu]
    linker = "x86_64-unknown-linux-gnu-gcc"
    ```
- 编译各个环境二进制并生成 NPM 包。
  ```bash
  node npm.js
  ```
  会在 `npm/` 目录下生成多个环境的 NPM 包。
- 项目根执行 `node publish.js` 即可全部发包。（不能用 `pnpm -r publish`，发出来的包会有权限问题）

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