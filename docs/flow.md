# Flow 编写

flow.oo.yaml 文件的 schema，可以访问 [oomol-schema](https://github.com/oomol/oomol-schema) 获取，或者访问公开构建好的 [flow-schema](https://static.oomol.com/json-schemas/Flow.schema.json)。

## node 中的路径
### task block

```yaml
nodes:
    - node_id: xxx
      task: <path>
```

`<path>` 有以下几种形式（代码位置`manifest_reader/src/block_manifest_reader/block_manifest_finder.rs`的`resolve_block_manifest_path`）；

1. `self::<block_name>` 从 flow.oo.yaml 所在目录的隔壁 blocks 目录查找 <block_name>/block.oo.yaml 文件；
2. `<absolute_path>` 绝对路径，直接使用绝对路径查找，不建议使用；查找逻辑 `<absolute_path>/block.oo.yaml 文件。
3. `<pkg>::<name>` 从传入的 block-search-paths 中查找，查找逻辑 `<block-search-path>/<pkg>/<name>/block.oo.yaml 文件。 block-search-path 为 block-search-paths 中的每一个元素。
4. `<relative_path>` 相对路径，查找逻辑 `<flow.oo.yaml 所在目录>/<relative_path>/block.oo.yaml 文件。

### service block

代码在 manifest_meta/src/block_reader.rs 的 resolve_service_node_block 方法里。

```yaml
nodes:
    - node_id: xx
      service: <pkg>::<service_name>::<method>
```