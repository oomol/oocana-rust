# debug

1. 安装 rust-analyzer 之外的插件
根据 [vscode 文档](https://code.visualstudio.com/docs/languages/rust#_debugging) 在不同系统上，安装对应的插件。macos/Linux 是 [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) 目前配置项也是用的 CodeLLDB，没有在 windows 上经过测试。

2. 配置 flow file 的路径
在`Run and Debug`中，选择`Debug executable 'oocana'`，会弹出提示框，输入 yaml 的地址。默认地址是`examples/base/flow.oo.yaml`。如果觉得麻烦，可以临时 configurations 中 name 为`Debug executable 'oocana'`的配置项中的第一级`args`字段中的`${input:flowFile}`更改为固定值，就不会在每次执行时，弹出输入框。

>运行时，需要使用按键 UI 交互，使用快捷键交互时，断点不生效。