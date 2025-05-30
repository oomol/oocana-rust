# layer

https://github.com/oomol/ovmlayer 的 rust 封装，只能运行在**满足条件的 linux 环境**中。

在非 linux 系统开发，需要使用 devcontainer 配置开发环境，devcontainer 的安装参考 [devcontainer 文档](https://code.visualstudio.com/docs/devcontainers/containers)。

## 项目准备

* rootfs 文件

在 https://github.com/oomol/ovmlayer-rootfs/releases 寻找 base-rootfs 开头的 release。这是 ovmlayer 所需要的基础 linux 环境文件（oocana 会在 ovmlayer 下运行 zsh，因此额外添加了 zsh）。
创建 `~/oomol/layer_blocker` 目录，用于存放 ovmlayer 所需要的 rootfs 以及 ovmlayer 自动创建的 disk。
将根据架构下载的 base-rootfs 移动到 `~/oomol/layer_blocker` 目录下，并重命名为 `base-rootfs.tar`。
> 确保根目录下 `.devcontainer/devcontainer.json` 中的 mount 的路径文件都真实存在，否则 build devcontainer 的时候，会报错。

对应的 shell 操作：

```shell
mkdir -p ~/oomol/layer_blocker

arch=$(uname -m)
# 只有 amd64 和 arm64。
echo "download $arch rootfs"
if [ $arch != "arm64" ]; then
    arch="amd64"
fi

curl -o base-rootfs.tar -L https://github.com/oomol/ovmlayer-rootfs/releases/download/base-rootfs%400.4.0/$arch-rootfs.tar
mv base-rootfs.tar ~/oomol/layer_blocker
```

* 多项目开发

> ovmlayer 会占用 docker 创建的 /dev/loop 设备。在多个项目使用 ovmlayer 环境时，会抢占 /dev/loop 设备，导致无法使用。所以目前最好只开一个使用 ovmlayer 的项目。
另外，这种情况下，在退出使用 ovmlayer 的项目时，需要手动调用 `losetup -D` 卸载当前容器对 /dev/loop 设备的占用。然后使用 `losetup -a` 查看是否还有其他占用，如果有，需要手动卸载。

> 由于 devcontainer 生命周期的问题，有时候 ovmlayer 启动会有问题，此时需要再次运行 `.devcontainer/setup.sh`。

* 容器内操作

## 配置

```JavaScript
//  ~/.oomol-studio/oocana/layer.json
{
    "base_rootfs": ["layer1", "layer2"]
}
```

目前在合并 layer 创建 merge point 时，会读取 `~/.oomol-studio/oocana/layer.json` 中的 base_rootfs 字段，将其中所有的 layers 作为 merge layer 时的基础 layer（作为最底部的 layer）。

所以 base rootfs 里除了需要 ovmlayer 必须的依赖以外，还需要有 python-executor 和 nodejs-executor 。否则在调用对应的 block 时，无法运行对应的 block。

> TODO: 可以将 python-executor 和 nodejs-executor 写到 base_rootfs 里，这样可以降低 rootfs 的大小，减少更新频率。

## ovmlayer 配置

在运行 ovmlayer 时，可以设置 `OVMLAYER_LOG` 环境变量，控制 ovmlayer 的日志输出位置。路径需要为绝对路径；如果是相对路径，无法保证路径正确。