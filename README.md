# Vocana Rust

![github-actions](https://github.com/oomol/vocana-rust/actions/workflows/build-and-test.yml/badge.svg?branch=main)

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
1. 产物在 `target/release/vocana`。

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
  
  brew tap messense/macos-cross-toolchains
  brew install x86_64-unknown-linux-gnu
  brew install aarch64-unknown-linux-gnu
  ```
- 先更新 `Cargo.toml` 中的版本。
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
  基于 Rust 实现的 Vocana block sdk，供本项目测试使用
- `src`
  Rust 程序入口
- `tests`
  本项目测试
- `utils`
  本项目可复用的一些工具方法
- `npm`
  把不同环境的二进制做成 NPM 包

### 执行过程

从整个程序角度看，当命令行输入 `vocana run ./[path_to_flow_project]` 时：

1. 先从 `src` 启动程序。
1. 通过 `cli` 处理命令行参数。
1. 然后执行 `core` 开始调度。

从 `core` 角度看，它的执行过程为:

1. 通过 `proj_reader` 读取图和 block 相关信息并构建内部图结构。
1. 找到图中的起始 blocks 全部执行（`block.json` 的 `cmd` 命令）。
1. 通过 `mainframe` 启动通讯服务等待消息。
1. 根据不同消息进行调度。

当 block 中的任务被执行后

1. sdk 会先启动，告知 vocana 服务 block 已启动，同时返回输入数据。
1. sdk 处理好后执行具体业务代码。
1. 产出结果告知 vocana 服务。
