# 2.0.0 版本边界独立检查

日期：2026-09-28。检查任务：`09-26-sync-diagnostics-contracts`。

范围：已批准的 2.0.0 开发树版本、双语迁移说明、相关规范、桌面测试格式，以及新的完整 `just ci`。未执行提交、发布、安装或真实数据 sync/rebuild/reset。

状态：版本与 API 迁移检查通过；完整 `just ci` 返回 0，耗时 276.111 秒。该检查范围内没有未解决的门禁失败。

## Findings (fixed)

- 文件：`desktop/src-tauri/tests/quota.rs`。
  问题：import、`spawn_local_quota_server` 签名和返回 tuple 共三处格式与 Rust 2024 rustfmt 输出不一致。
  修复：使用 manifest 指定的 edition 2024 重新格式化。`cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check` 返回 0。未修改测试语义。

## 迁移说明与公开 API

已逐项核对 `docs/reference/migration-v2.md` 与 `docs/zh/reference/migration-v2.md`。两份文档的 API 表和 Rust 示例一致。

- 14 个结构体：从当前源码读取字段，并与 `git show v1.2.0:<path>` 的原定义比较。新增字段集合完全匹配迁移表。`ReportFilter` 的四个移出字段为 `since`、`source`、`timezone`、`until`，均由 `QueryFilter` 承接。
- 16 个方法：所属类型、方法名与参数顺序均匹配当前定义。`ReportCommonArgs::to_filter` 的 Store 参数在 project 参数之前；所有 host 参数位置正确。
- `QueryFilter` 和 `ReportTimezone` 由 crate 根导出，`query::reports::ReportFilter` 与 `store::LOCAL_HOST_ID` 为公开路径。`ReportFilter` 和 `QueryFilter` 均实现 `Default`。双语文档的原始代码块置于临时 `main` 函数中，使用当前构建的 llmusage rlib 和原生 rustc 编译 metadata，exit 0。
- `SourceKind` 新增 `AntigravityIde`、`Omp`；`PricingStatus` 新增 `SourceReported`；`SyncEvent` 新增三个远端主机 variant；`Commands` 新增两个 variant，Sync 增加两个字段。声明顺序、数值 discriminant 和 `SourceKind` 派生排序说明与历史 SemVer 日志、当前源码一致。
- `LossyRebuildRisk` 的 owned `host_id` 与移除 `Copy` 记录正确。`JsonlRecord` 的一基行号与 newline-terminated durable 边界说明匹配字段契约。
- `PiParser` 的两条旧导出路径由 `PiFormatParser::pi()` / `omp()` 承接；两种实例绑定各自 SourceKind 和文件发现函数。`PricingCatalog::embedded()` 返回 `&'static PricingCatalog`。`commands::dash::run(app, false).await` 匹配现有签名；CLI `tui` 分支仍传递 `true` 以显示弃用提示。
- `subscription::fetch_all` 在 v1.2.0 返回 `UsageFetchReport`，当前返回 `UsageFetchOutcome`。新类型含 `report` 和 `cache_hit`。只有成功读取未绕过的缓存时 cache_hit 为 true；缓存的 report payload 仍为 `UsageFetchReport`。迁移说明和 `tui-subscription-contracts.md` 均匹配实现。

结构体与方法的路径、行号及机器校验结果保存在 `version-boundary-api-review.json`。示例编译结果保存在 `version-boundary-example-result.json`。

主要源码证据：`src/query/reports.rs:29`、`src/query/filter.rs:29`、`src/commands/report_args.rs:76`、`src/store/source_file.rs:57`、`src/store/cursor.rs:24`、`src/store/sync_status.rs:29`、`src/store/schema.rs:345`、`src/parsers/pi.rs:91`、`src/domain/pricing_catalog.rs:261`、`src/commands/dash.rs:31`、`src/subscription/mod.rs:61`、`src/subscription/cache.rs:9`。

## 版本与依赖锁

准确数量为 13 个文件、15 个产品版本字段。两个额外字段来自桌面 Cargo lock 的两个本地 package 与 npm lock 的顶层/根 package 版本。所有字段均为 `2.0.0`。

独立解析 TOML/JSON，并与 HEAD 的完整锁结构比较。仅归一化已批准的本地产品版本字段；第三方版本、checksum/integrity、依赖边和其他属性全部相同。

| 锁文件 | 第三方 package 数 | 与 HEAD 结构比较 |
| --- | ---: | --- |
| `Cargo.lock` | 421 | 相同 |
| `desktop/src-tauri/Cargo.lock` | 676 | 相同 |
| `desktop/package-lock.json` | 226 | 相同 |
| `docs/package-lock.json` | 173 | 相同，完整解析结构无变化 |

三个本地 Cargo package 的非版本字段及 npm 根 package 的非版本字段也保持相同。独立结果、全部字段和值、第三方结构 SHA-256 保存在 `version-boundary-independent.json`。

## 正式 SemVer

正式命令保持 `cargo semver-checks --baseline-rev v1.2.0`。基线 commit 独立核对为 `9b7a6f3dec12764222891c2d8f5aeb42db7bd490`。

`semver-2.0.log` 与 `semver-2.0.exit` 记录 exit 0、`v1.2.0 -> v2.0.0 (major change)`、`0 checks: 0 pass, 254 skip`、`no semver update required`。工具计时 45.196 秒。该结果证明版本边界允许现有 API 变化；兼容性规则全部因 major 跳过，结果不证明与 v1.2.0 保持源码兼容。

首次在线尝试的 schannel TLS 失败仍保存在 `semver-2.0-online-failure.log`。随后只设置进程级 `CARGO_NET_OFFLINE=true` 并保持正式命令，使用已缓存 registry 成功完成。未变更 baseline、release-type 或 lint policy。历史 1.4.0 失败日志继续保留。

## 完整门禁

新运行使用原始 `just ci`。命令级 PATH 将 `C:/Users/lyh/.cargo/bin` 放在首位，并先断言 `Get-Command cargo` 为原生 cargo.exe。命令环境写入 `ci-v2-env.json`。完整输出写入 `ci-v2.log`。

执行前四份锁文件 SHA-256 写入 `ci-v2-locks-before.json`；结束后保存 `ci-v2-locks-after.json` 和 `ci-v2-result.json`。四个文件在完整门禁前后均保持字节相同。

| 检查 | 结果 |
| --- | --- |
| CI gate self-test 与契约检查 | PASS |
| 根 Rust fmt / Clippy `-D warnings` / rustdoc | PASS |
| 根 Rust lib | 905 passed，12 ignored，0 failed |
| 八个集成测试 target | 246 passed，0 failed；含 sync 143、CLI 34 |
| dashboard JS | PASS |
| 桌面前端 | 18 files，65 tests passed；TypeScript 与 Vite 构建通过 |
| 桌面 Rust | lib 18、AC 9、quota 6 passed，0 failed |
| VitePress 文档 | PASS |
| `just ci` 总结果 | exit 0，276.111 秒 |

原生 Cargo 使用命令范围 PATH；未改仓库门禁参数或全局配置。Windows linker 的 `.lib` / `.exp` 提示仍产生 `linker_messages` warning，测试构建成功。额外的 migration 示例 metadata 编译返回 0。Cargo 槽已经释放。

## Findings (not fixed)

当前版本和迁移说明检查未发现需要修复的事实错误。完整 CI 通过。先前 MBX wrapper access violation 与间歇 CLI 日志断言的原因未查明；历史失败保留。本轮原生 Cargo 的完整 CLI suite 34/34 和 sync suite 143/143 通过，不能据此推断旧失败的原因。
