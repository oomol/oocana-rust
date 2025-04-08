##  configuration

oocana supports loading configuration from files. The configuration file formats can be `toml`, `json`, or `json5`. By default, it will look for configuration files `config.toml`, `config.json`, or `config.json5` under `~/.oocana/`. If none are found, the default configuration will be used. You can also specify the configuration file path using the `--config` parameter.

Currently, the config supports the following options:

* Global-level configuration options:

- `store_dir`: Directory for saving some globally shared storage, default is `~/.oomol-studio/oocana`.
- `oocana_dir`: Directory for a single run of oocana, default is `~/.oocana`.
- `env_file`: Path to the env file used when running flows or creating layers. No default value. It can be overridden by the `OOCANA_ENV_FILE` environment variable or the `--env-file` CLI parameter. (Only used in the Run and Layer functionalities.)
- `bind_path_file`: Path to the file that reads `bind_paths` when using the Layer functionality. No default value. It can be overridden by the `OOCANA_BIND_PATH_FILE` environment variable or the `--bind-path-file` CLI parameter. (Only used in the Run and Layer functionalities.)
- `search_paths`: An array of paths used to search for packages. No default value.

* Run flow configuration:

- `broker`: MQTT broker address, default is `127.0.0.1:47688`.
- `exclude_packages`: An array of package paths to exclude when running flows. No default value.
- `reporter`: Whether to enable the reporter when running flows, default is `false`.
- `debug`: Whether to run in debug mode, default is `false`.
- `extra`: Additional configuration options that can be added as needed. Currently, there is only one extra option, `search_paths`, which specifies paths to search for packages during flow execution. This option cannot be overridden and is always the last search location.

## 配置项

oocana 支持从文件加载配置。配置文件的格式可以为 toml、json、json5 三种文件后缀，默认会在 `~/.oocana/` 下查找配置文件 `config.toml`, `config.json`, `config.json5`。如果没有找到，则会使用默认配置。你也可以通过 `--config` 参数指定配置文件的路径。

目前 config 支持以下配置项：

* global 级别配置项：

- store_dir: 保存部分全局公用的存储，默认为 `~/.oomol-studio/oocana`
- oocana_dir: oocana 单次运行时的目录，默认为 `~/.oocana`
- env_file: 运行 flow，创建 layer 时，使用的 env 文件路径。不存在默认值。会被 OOCANA_ENV_FILE 环境变量和 cli 参数 `--env-file` 覆盖。（仅在 Run 和 layer 功能中使用）
- bind_path_file: 使用 layer 功能时，读取 bind_paths 的文件路径，不存在默认值。会被 OOCANA_BIND_PATH_FILE 环境变量和 cli 参数 `--bind-path-file` 覆盖。（仅在 Run 和 layer 功能中使用）
- search_paths: 用于搜索 package 的查找路径，为数组，不存在默认值。

* Run flow 配置

- broker mqtt broker 地址，默认为 `127.0.0.1:47688`
- exclude_packages: 运行 flow 时，排除的 package 路径，为数组，不存在默认值。
- reporter: 运行 flow 时，是否开启 reporter，默认为 false
- debug: 是否以 debug 模式运行，默认值为 false
- extra: 额外的配置项，可以根据需要添加，目前只有一个额外的 search_paths 配置项，表示在运行 flow 时，在 search_paths 中查找 package 的路径。不会被覆盖。同时永远处于最后一个查找位置。