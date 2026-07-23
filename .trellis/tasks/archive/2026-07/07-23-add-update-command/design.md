# 自更新命令技术设计

## Architecture

新增 `src/commands/update.rs`，由该模块独占更新渠道、命令构造、交互确认和
外部进程执行。`src/commands/mod.rs` 只负责 Clap 参数声明和 dispatch 路由，
不把自更新逻辑放入 `AppContext`、Store 或集成层。

公开命令形态：

```text
llmusage update [OPTIONS] [CHANNEL]

CHANNEL: main (default) | dev
OPTIONS: --check, -c
```

`UpdateChannel` 使用 Clap `ValueEnum`，从类型层限制渠道为 `main` / `dev`。
默认值为 `main`，命令帮助明确 `dev` 是开发渠道。

## Command Contract

唯一仓库常量为 `https://github.com/bahayonghang/llmuasage`，唯一包名常量为
`llmusage`。安装参数由纯函数构造，顺序固定为：

```text
cargo install --git <repo> llmusage --branch <channel> --locked --force
```

`--check` 输出版本、仓库、渠道和上述命令后返回，不读取确认输入，也不创建
子进程。实际更新先显示同样的信息，然后读取一行确认：空行、`y`、`yes`
继续；`n`、`no` 取消并以成功状态退出；其他输入重新询问；EOF/I/O 错误返回
错误且不执行 Cargo。

Cargo 子进程继承 stdout/stderr/stdin，使下载和编译进度保持实时可见。启动
失败或非零退出状态转换为 `anyhow` 错误，错误包含渠道、退出码（若有）和
完整人工复现命令。该命令不写 Store/run_log，避免更新过程依赖数据库状态。

## Test Seams

模块内部把以下边界拆成小函数，而不新增生产依赖：

- `install_args(channel)`：返回确定的参数列表。
- `confirm_update(reader, writer)`：对注入的 `BufRead` / `Write` 测试确认状态机。
- `run_with(channel, check, reader, writer, executor)`：执行器闭包接收命令规范，
  单元测试记录调用或返回模拟成功/失败；生产入口使用 `std::process::Command`。

测试不会修改全局 `PATH`，不会启动网络，也不会触碰真实 Cargo 安装目录。
Clap 解析与帮助可继续放在 `src/commands/mod.rs` 的现有测试模块中。

## Documentation

更新 `README.md`、`README.zh-CN.md`，并在现有安装指南
`docs/guide/install-and-init.md`、`docs/zh/guide/install-and-init.md` 增加自更新
小节。文档同时覆盖默认 `main`、`dev`、`--check`、确认行为、Rust/Cargo
前置条件和开发渠道风险，不新建重复导航页面。

## Compatibility And Safety

- 不改变无子命令默认 daily 行为、现有全局 `--home` 或库 façade。
- 不新增 `LlmusageError` 公共变体；CLI 外部进程失败使用内部 `anyhow` 上下文。
- 自替换、旧二进制保留和安装清理由 Cargo 自己负责。本任务不实现 Release
  下载器或自定义二进制交换协议；Cargo 失败时如实返回，不声称自定义回滚。
- Windows、Linux、macOS 共用 Cargo 安装路径，不引入 shell 字符串拼接，
  所有参数通过 `Command::args` 传递。

## Risks And Rollback

- `dev` 可能无法编译或不稳定：命令和文档显式标记风险，Cargo 非零即失败。
- 用户没有 Cargo 或无法访问 GitHub：启动/网络/编译失败保留人工复现命令。
- 运行中的可执行文件是否能被替换取决于 Cargo 和平台：沿用 CCR 已采用的
  机制，不添加未经要求的后台 helper；失败状态不得被包装成成功。
- 回滚代码改动时，移除 `update.rs`、命令枚举/dispatch 分支、测试和文档小节
  即可；用户数据格式没有迁移或回滚步骤。
