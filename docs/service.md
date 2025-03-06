# service

相关飞书讨论 https://oomol-studio.feishu.cn/wiki/CbhTwpHBJiplFDkm6lncWvxDnEc

## 基础定义

service 是 package 下，一组 block 的集合，这些 block 会交由一个进程处理，这个进程就是 service 模块。

service 意义在 package 的 services 文件夹下，services 文件夹下每一个子文件夹，即为一个 service，同时文件夹名称即为 service 的名称。文件夹下会存在一个 service.oo.yaml 文件，用来描述 service 的基本信息。

以下为`service.oo.yaml`的示例：

```yaml
executor:
  name: python
  entry: ./main.py
  function: main
blocks:
  - name: a
    outputs_def:
      - handle: two
  - name: b
    inputs_def:
      - handle: one
    outputs_def:
      - handle: two
```

在 flow 中，使用的 service.oo.yaml 的 blocks 而不是 block.oo.yaml。这种 block，在这里称之为 `service block`，使用方法示例：

```yaml
nodes:
  - node_id: 1
    service: <pkg>::<service_name>::<block_name>
    # 其他参数类似普通 block 
```