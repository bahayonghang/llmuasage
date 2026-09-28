# 2.0.0 版本边界实施证据

日期：2026-09-28。任务：`09-26-sync-diagnostics-contracts`。

用户已选择推荐的 2.0.0 开发树版本边界。版本实施仅修改下列 13 个产品版本文件；迁移文档、CHANGELOG、规范和任务验收由主代理负责。保留了开始执行前已有的诊断和文档修改。

## 版本一致性

以下 13 个文件共 15 个产品版本字段均为 `2.0.0`；原值均为 `1.4.0`。

| 文件 | 校验入口 | 当前值 |
| --- | --- | --- |
| `Cargo.toml` | `package.version` | `2.0.0` |
| `Cargo.lock` | `llmusage` package | `2.0.0` |
| `desktop/src-tauri/Cargo.toml` | `package.version` | `2.0.0` |
| `desktop/src-tauri/Cargo.lock` | `llmusage`、`llmusage-desktop` packages | 两项均为 `2.0.0` |
| `desktop/src-tauri/tauri.conf.json` | `version` | `2.0.0` |
| `desktop/package.json` | `version` | `2.0.0` |
| `desktop/package-lock.json` | 顶层 `version`、`packages[""].version` | 两项均为 `2.0.0` |
| `README.md` | 第 9 行当前 crate 版本 | `2.0.0` |
| `README.zh-CN.md` | 第 9 行当前 crate 版本 | `2.0.0` |
| `docs/index.md` | 第 51 行版本 | `2.0.0` |
| `docs/zh/index.md` | 第 51 行版本 | `2.0.0` |
| `docs/reference/cli.md` | 第 3 行文档版本 | `2.0.0` |
| `docs/zh/reference/cli.md` | 第 3 行文档版本 | `2.0.0` |

版本字段由 Python `tomllib` / `json` 与六个准确文档行独立读取并断言。`cargo metadata --locked` 同时确认根 crate 和桌面 crate 均为 `2.0.0`。根 crate 的 8 个集成测试 target 保持 `api`、`architecture_dependencies`、`cli`、`query`、`remote`、`store`、`sync`、`tui`。

## 写入命令与锁文件保护

每个 Cargo 命令使用命令范围 PATH，首项为 `C:/Users/lyh/.cargo/bin`。`Get-Command cargo` 解析为该目录中的原生 `cargo.exe`；工具输出为 `cargo 1.97.0 (c980f4866 2026-06-30)`。没有修改全局 PATH。

已完成的写入命令：

```text
just version-sync 2.0.0
npm --prefix desktop version 2.0.0 --no-git-tag-version --ignore-scripts
cargo metadata --offline --format-version 1 --manifest-path desktop/src-tauri/Cargo.toml
```

`just version-sync` 同步根 manifest、根 lock 和六份文档，内部使用 `cargo update --offline --package llmusage`。桌面 manifest 与 Tauri JSON 的产品版本字段采用最小补丁。npm 同步桌面 package 与 package-lock 的本地版本字段，未执行 package scripts 或 Git tag。桌面 Cargo lock 最终由上述 `cargo metadata` 命令更新。

首次尝试的 `cargo update --offline --manifest-path desktop/src-tauri/Cargo.toml --package llmusage --package llmusage-desktop` 返回 0，但改变了 Windows 依赖边并移除了 `windows-core 0.62.2`、`windows-result 0.4.1`、`windows-strings 0.5.1`。该结果超出范围，未保留。恢复时先确认 HEAD 内容采用原 CRLF 换行后的 SHA-256 与执行前文件完全相同，再恢复该次本代理修改，随后用 Cargo metadata 重新生成本地包版本项。没有手工修改或拼接依赖锁项。

执行前完整锁结构保存在临时文件 `llmusage-version-boundary-20260928-before.json`。最终比较排除本地 package 的版本字段，对所有第三方 package 的完整结构进行等值校验；包括 package 版本、source、checksum/integrity、依赖边和 npm 包属性。

| 锁文件 | 第三方项数（前 / 后） | 完整结构结果 | 前后相同的规范化 SHA-256 |
| --- | ---: | --- | --- |
| `Cargo.lock` | 421 / 421 | 相同 | `85a1394865e3af41eab56a48c91741512907be8dfcdac3a4919c914ba77e3faf` |
| `desktop/src-tauri/Cargo.lock` | 676 / 676 | 相同 | `b58fc28b7832e7e2163de906f1fee93199a8dc46947d792011a8d3514887d1a1` |
| `desktop/package-lock.json` | 226 / 226 | 相同 | `44187bfcd357a6d03142aa310d10f8dd4e2221b4b67ee98b59ee81b4f6b22467` |

规范化使用 `json.dumps(data, sort_keys=True, separators=(',', ':'))` 的 UTF-8 字节。Cargo 列表保留原顺序；npm 按 package path 键序列化。三个本地 Cargo package 的非版本字段也完全相同。npm lock 仅两个本地版本字段变化。`docs/package-lock.json` 与执行前字节完全相同。第三方 `finl_unicode 1.4.0` 和 `expect-type 1.4.0` 保留。

## 定向验证

| 命令 | 结果 |
| --- | --- |
| `cargo metadata --locked --all-features --no-deps --format-version 1` | exit 0；根 crate `2.0.0`，8 个集成 target |
| `cargo metadata --locked --no-deps --format-version 1 --manifest-path desktop/src-tauri/Cargo.toml` | exit 0；桌面 crate `2.0.0` |
| `cargo fmt --all --check` | exit 0 |
| `node desktop/node_modules/typescript/bin/tsc --noEmit --project desktop/tsconfig.json` | exit 0 |
| `git diff --check -- <上述 13 个文件>` | exit 0；仅 Git LF/CRLF 提示 |
| `cargo test --locked --all-features --test api -- --test-threads=1` | exit 0；3 passed，0 failed；编译 28.33 秒，测试 1.01 秒 |

API 输出保存在 `version-boundary-api.log` 与 `version-boundary-api.exit`。Windows linker 输出创建 `.lib` / `.exp` 的信息并产生 `linker_messages` warning；构建与测试均成功。

补充执行 `cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --all --check` 返回 1：现有 `desktop/src-tauri/tests/quota.rs` 的 import、函数签名和 tuple 返回有 3 处格式差异。该文件不在本代理所有权内，未修改；已通知主代理。该补充命令不属于当前 `just ci` 的 desktop 步骤。

## 正式 SemVer

`git rev-parse "v1.2.0^{commit}"` 已核对为 `9b7a6f3dec12764222891c2d8f5aeb42db7bd490`。使用已安装的临时工具 `cargo-semver-checks 0.50.0`；命令范围 PATH 中，原生 Cargo 目录在该工具目录之前。

```text
cargo semver-checks --baseline-rev v1.2.0
```

首次执行返回 101，耗时 41.119 秒。在生成 current crate rustdoc 的准备步骤，scaffold 的 `cargo update` 获取 crates.io `config.json` 时出现 `schannel` TLS handshake 失败，未进入 API 比较。完整输出保存在 `semver-2.0-online-failure.log` 与 `semver-2.0-online-failure.exit`。

重试只在该进程设置 `CARGO_NET_OFFLINE=true`，使用已缓存的 registry 依赖；原始正式命令保持。工具成功构建、解析当前 `2.0.0` 与基线 `1.2.0`，返回 0，输出如下：

```text
Checking llmusage v1.2.0 -> v2.0.0 (major change)
Checked [   0.000s] 0 checks: 0 pass, 254 skip
Summary no semver update required
Finished [  45.196s] llmusage
```

进程总耗时 46.346 秒；工具报告 45.196 秒。最终结果保存在 `semver-2.0.log` 与 `semver-2.0.exit`。历史 v1.4.0 失败日志和此次网络失败均保留。没有传入 `--locked`、`--release-type` 或 lint 覆盖参数。该通过表示实际 package major 已满足已批准的 API 版本边界；254 项兼容性检查因 major 变更跳过，不表示恢复 v1.2.0 源码兼容，也不替代迁移说明和行为测试。

SemVer 结束后再次校验四个锁文件。三个产品锁文件只含前述本地版本变化；docs lock 字节不变。以下最终字节 SHA-256 可供主代理后续完整门禁比较：

| 文件 | SHA-256 |
| --- | --- |
| `Cargo.lock` | `f88651176c1d0f1b64b949f697217a95e23b3cdb18cef7b1c0b09401a7dce9ba` |
| `desktop/src-tauri/Cargo.lock` | `93dd2b3a506284617486806b5a0e375362c396bd77545cf1ab506b5e5e64b358` |
| `desktop/package-lock.json` | `95378d7b1409b56b957d325f78ad2ba2b879766ef2fa31ab42a5de72ef312ad5` |
| `docs/package-lock.json` | `9a4fe421531cda95ffe058f3a66b7b917ab519bea194881818f62a9ba2bf7173` |

## 验证边界

未修改公开 API 实现、SQLite schema、accounting、remote wire 版本、正式 baseline、CI lint 策略或发布工具。未执行 tag、commit、release、安装或真实 sync/rebuild/reset。完整 `just ci`、文档构建和独立检查由主代理在迁移文档完成后统筹。
