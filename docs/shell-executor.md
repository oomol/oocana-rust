# Shell Executor

- [English](#english)
- [中文](#中文)

---

## English

### Overview

`ShellExecutor` is one of the four task block executors (NodeJS, Python, Shell, Rust), used to execute shell commands directly. It is the **simplest** of all executors - no configuration options at all.

### YAML Configuration

```yaml
# Minimal configuration
executor:
  name: shell

# Full configuration (as task block)
type: task_block
executor:
  name: shell
inputs_def:
  - handle: command      # Required: command to execute
  - handle: cwd          # Optional: working directory
  - handle: envs         # Optional: environment variables
outputs_def:
  - handle: stdout       # Standard output
  - handle: stderr       # Standard error output
```

### Input Parameters

| handle | type | required | description |
|--------|------|----------|-------------|
| `command` | string | Yes | Shell command to execute |
| `cwd` | string | No | Working directory (supports relative and absolute paths) |
| `envs` | string | No | Environment variables, format: `KEY1=VALUE1,KEY2=VALUE2`. Values may contain `=` (e.g., `KEY=a=b=c`), but commas are used as separators and cannot be escaped. Invalid pairs are silently ignored. |

### Output Parameters

| handle | type | description |
|--------|------|-------------|
| `stdout` | string | Command's standard output (accumulated line by line) |
| `stderr` | string | Command's standard error output (accumulated line by line) |

### Runtime Mechanism

1. Commands are executed via `sh -c "<command>"`
2. Environment variables auto-injected: `OOCANA_SESSION_ID` and `OOCANA_JOB_ID`
3. Non-zero exit code causes block to fail
4. stdout/stderr are reported line by line in real-time, sent as output upon completion

### Examples

**Standalone block definition** (`block.oo.yaml`):
```yaml
type: task_block
executor:
  name: shell
inputs_def:
  - handle: command
    value: "ls -al"
outputs_def:
  - handle: stdout
```

**Inline usage in flow**:
```yaml
nodes:
  - node_id: list-files
    task:
      executor:
        name: shell
      inputs_def:
        - handle: command
      outputs_def:
        - handle: stdout
    inputs_from:
      - handle: command
        value: "ls -l"
```

### Comparison with Other Executors

| Feature | Shell | NodeJS/Python | Rust |
|---------|-------|---------------|------|
| Config options | None | entry, function, spawn | bin, args |
| Spawn mode | No | Yes | No |
| Output method | stdout/stderr handle | SDK active send | SDK active send |

**Key difference**: Shell executor's output is **passively captured** (via stdout/stderr), while NodeJS/Python require actively calling SDK APIs to send output.

---

## 中文

### 概述

`ShellExecutor` 是四种 task block 执行器之一（NodeJS、Python、Shell、Rust），用于直接执行 shell 命令。它是所有执行器中**最简单**的一种——没有任何配置选项。

### YAML 配置格式

```yaml
# 最小配置
executor:
  name: shell

# 完整配置（作为 task block）
type: task_block
executor:
  name: shell
inputs_def:
  - handle: command      # 必需：要执行的命令
  - handle: cwd          # 可选：工作目录
  - handle: envs         # 可选：环境变量
outputs_def:
  - handle: stdout       # 标准输出
  - handle: stderr       # 标准错误输出
```

### 输入参数

| handle | 类型 | 必需 | 说明 |
|--------|------|------|------|
| `command` | string | 是 | 要执行的 shell 命令 |
| `cwd` | string | 否 | 工作目录（支持相对路径和绝对路径） |
| `envs` | string | 否 | 环境变量，格式：`KEY1=VALUE1,KEY2=VALUE2`。值可以包含 `=`（如 `KEY=a=b=c`），但逗号作为分隔符不可转义。无效的键值对会被静默忽略。 |

### 输出参数

| handle | 类型 | 说明 |
|--------|------|------|
| `stdout` | string | 命令的标准输出（逐行累积） |
| `stderr` | string | 命令的标准错误输出（逐行累积） |

### 运行机制

1. 命令通过 `sh -c "<command>"` 执行
2. 自动注入环境变量：`OOCANA_SESSION_ID` 和 `OOCANA_JOB_ID`
3. 非零退出码会导致 block 报错
4. stdout/stderr 实时逐行上报日志，完成后作为输出发送

### 示例

**独立 block 定义** (`block.oo.yaml`):
```yaml
type: task_block
executor:
  name: shell
inputs_def:
  - handle: command
    value: "ls -al"
outputs_def:
  - handle: stdout
```

**在 flow 中内联使用**:
```yaml
nodes:
  - node_id: list-files
    task:
      executor:
        name: shell
      inputs_def:
        - handle: command
      outputs_def:
        - handle: stdout
    inputs_from:
      - handle: command
        value: "ls -l"
```

### 与其他执行器的区别

| 特性 | Shell | NodeJS/Python | Rust |
|------|-------|---------------|------|
| 配置选项 | 无 | entry, function, spawn | bin, args |
| spawn 模式 | 不支持 | 支持 | 不支持 |
| 输出方式 | stdout/stderr handle | SDK 主动发送 | SDK 主动发送 |

**关键区别**：Shell 执行器的输出是**被动捕获**的（通过 stdout/stderr），而 NodeJS/Python 需要通过 SDK 主动调用 API 发送输出。
