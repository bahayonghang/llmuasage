# Research: SemVer 门禁与 2.0 API 版本边界

- Query: 保留新来源、host/remote、当前 accounting 和 CLI 行为，确定正式 `cargo semver-checks --baseline-rev v1.2.0` 门禁的修复方案。
- Scope: internal；现有日志、当前源码、正式检查生成的 v1.2.0 源码快照、ADR、版本同步入口。只写本任务 research。
- Date: 2026-09-27（本轮 PowerShell `Get-Date`）。
- Authorization: 用户已授权“先扩大范围修复这些门禁，然后继续”；本研究不执行版本、代码、规范、Git 或发布变更。
- Method: 按 su-architecture-first 核对目标、所有权、根因、版本边界和验证证据。

## Findings

### 建议与材料性选择

建议将已接受的 API 变化纳入明确的 **2.0.0 开发树版本边界**，补齐迁移说明，并保持正式命令和 v1.2.0 baseline 不变。该方案保留当前来源、host、成本和 CLI 语义。版本准备不包含创建 tag、发布、安装产品或操作真实 usage 数据。

当前公开类型无法通过少量兼容函数同时保留 v1.2.0 的完整源码兼容性和目前新增的来源/远端事件。基线的相关枚举允许穷尽匹配；14 个相关结构允许公开字段字面量构造。新增 variant/字段已经改变编译契约。

| 选择 | 可观察结果 | 工作边界 |
| --- | --- | --- |
| 2.0.0 版本边界（建议） | Rust 库调用者按迁移说明更新源码；现有 CLI、新来源及 host 行为保留 | 同步实际版本入口，列明已知 API 破坏，验证正式门禁和行为；不发布 |
| 维持 1.x 源码兼容 | v1.2.0 调用方继续使用原始类型、构造式、方法和穷尽 match | 冻结旧 API 图，建立独立新类型/扩展 API，定义旧 API 对新来源/host 的投影，验证两套边界 |

用户需要选择一次上述具体结果。已有授权允许门禁修复继续，不需要重复一般实施批准；但选择 2.0 下游迁移与维护 1.x façade 会产生不同结果。CI 契约 `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md:80–87` 对 shim/版本提升要求“without a separate approval”，第 149 行禁止在门禁中消音或包裹失败。主会话可以继续独立的诊断与时区修复，再针对明确版本方案询问一次。

### 证据与根因

- 当前任务 `semver-check.log`：v1.2.0 → v1.4.0，196 类检查中 184 pass、11 fail、1 warn、58 skip；末尾要求 major。符号因公开 re-export 重复出现，不能把重复输出算作独立产品问题。
- `semver-head-comparison.log`：v1.4.0 → v1.4.0，223 pass、31 skip。这证明当时诊断改动没有新增该工具可见的破坏，不能替代正式 v1.2.0 门禁。
- `Cargo.toml:1–15` 当前为版本 1.4.0 的 library + binary。`src/lib.rs:3–8,11–36,73–95` 明确根 façade，并继续公开 compatibility modules。`SourceKind`、`QueryFilter` 等破坏直接影响 façade。文档中的内部标签不会消除公开项目的 Rust 编译契约。
- `.github/workflows/ci.yml:193–201` 固定正式命令。CI 契约:29–31,80–91 固定 release baseline 与 SHA。根因属于已采用功能的 API/版本边界；没有证据支持修改为 HEAD、选 crates.io 同名项目、allow lint 或删掉门禁。
- `tests/api/facade.rs:8–36` 使用当前类型和 `..QueryFilter::default()`；没有验证 v1.2.0 的完整字段字面量和穷尽 match。

基线读取自此前正式工具生成的快照，其 Cargo.toml:3 为 1.2.0：

```text
BASELINE_ROOT = C:/Users/lyh/AppData/Local/mbx/targets/v1/43c669ae3236467fb96e4cdb1508bad29c8d456ef750d3bc51128ed39f5dfa21/semver-checks/git-v1_2_0/74fa9a2a58e69334f828b677643344b2d3577723
```

本研究未运行 Git 命令，遵守 researcher 的权限边界。baseline SHA 采用仓库契约与主会话已验证记录：`9b7a6f3dec12764222891c2d8f5aeb42db7bd490`。下文 BASELINE_ROOT 指上述只读源码快照。

### 正式门禁的 11 类失败

| 类别 | 去重符号与变化 | 影响/当前定位 |
| --- | --- | --- |
| `constructible_struct_adds_field` | 14 个 struct、19 个字段，见下表 | 旧完整字面量缺字段；serde default 不补齐 Rust 字面量 |
| `derive_trait_impl_removed` | `LossyRebuildRisk: Copy` 消失 | `src/store/source_file.rs:56–68` 新增 owned `String host_id` |
| `enum_no_repr_variant_discriminant_changed` | `PricingStatus::Unpriced` 2→3；`SyncEvent::{Finished,Failed,Cancelled}` 16/17/18→19/20/21；`SourceKind::{KimiCode,Pi,Grok,Zcode,DeepseekHarness}` 4/5/6/7/8→5/6/8/9/10 | 声明顺序变化；SourceKind 还触发 `PartialOrd` 顺序 warning |
| `enum_struct_variant_field_added` | `Commands::Sync::{emit_shards,since}` | `src/commands/mod.rs:104–118` |
| `enum_variant_added` | `Commands::{AntigravityIde,Remote}`；`PricingStatus::SourceReported`；`SourceKind::{AntigravityIde,Omp}`；`SyncEvent::{RemoteHostStarted,RemoteHostFinished,RemoteHostSkipped}` | 4 个 exhaustive enum 共 8 个新 variant |
| `function_missing` | `commands::tui::run` | 原 BASELINE_ROOT/src/commands/tui.rs:6；当前 `src/commands/dash.rs:31` 多 `deprecated: bool` |
| `inherent_method_missing` | `PricingCatalog::static_v1` | 原 BASELINE_ROOT/src/query/pricing_catalog.rs:271–275；当前 `src/domain/pricing_catalog.rs:261` 为 `embedded()` |
| `method_parameter_count_changed` | 16 个唯一方法，见下文 | 新增 host 或 Store 参数 |
| `module_missing` | `commands::tui` | CLI hidden tui alias 仍由 dash 分发，不恢复 Rust 模块路径 |
| `struct_missing` | `parsers::pi::PiParser` / `parsers::PiParser`（同一类型） | 原 unit struct；当前 `src/parsers/pi.rs:91–109` 为 `PiFormatParser`，`src/parsers/mod.rs:34` re-export |
| `struct_pub_field_missing` | `ReportFilter::{since,until,timezone,source}` | `src/query/reports.rs:29–35` 归入 `filter: QueryFilter`；Deref 不恢复构造式 |

| 当前 struct | 新增字段 | 当前源码锚点 |
| --- | --- | --- |
| `JsonlRecord` | `line_number`, `durable` | `src/parsers/file_state.rs:142–149` |
| `DashboardSnapshot` | `hosts` | `src/query/snapshot.rs:16–34` |
| `DashboardCoreSnapshot` | `hosts` | 同文件:80–98 |
| `DashboardInteractiveSnapshot` | `hosts` | 同文件:111–117 |
| `SourceCapabilityStatus` | `accounting` | `src/commands/source_status.rs:19–31` |
| `ReportFilter` | `filter` | `src/query/reports.rs:29–35` |
| `SyncShard` | `host_id`, `host_prefix_applied`, `opencode_cursor`, `zcode_cursor` | `src/store/mod.rs:796–847` |
| `HomeOverviewSeriesItem` | `antigravity_ide` | `src/query/home_overview.rs:59–64` |
| `TopSessionRow` | `first_event_at`, `last_event_at` | `src/query/top_sessions.rs:68–76` |
| `QueryFilter` | `host_id` | `src/query/filter.rs:29–41` |
| `ReportCommonArgs` | `host` | `src/commands/report_args.rs:19–72` |
| `UsageEvent` | `source_cost` | `src/domain/models.rs:329–352` |
| `DriveContext` | `sweep_host_ids` | `src/parsers/driver.rs:52–64` |
| `LossyRebuildRisk` | `host_id` | `src/store/source_file.rs:57–68` |

16 个方法的当前签名（省略 &self），括号内注明相对 v1.2.0 的新增参数：

- `ReportCommonArgs::to_filter(&store, project)`：新增 Store；`src/commands/report_args.rs:76–80`。
- `SourceFileStore::{counts,tracked_paths,lossy_rebuild_risk}(source, host_id)`：新增 host_id；`src/store/source_file.rs:92,98,157–161`。
- `SourceFileStore::sweep_missing(source, host_id, run_started_at)`：新增 host_id；同文件:118–123。
- `SourceFileStore::mark_inventory_seen(source, host_id, file_paths, seen_at)`：新增 host_id；同文件:139–145。
- `SyncStatusStore::load_source_sync_statuses(host_id)`：原无参数；`src/store/sync_status.rs:29`。
- `SyncStatusStore::save_source_sync_statuses(host_id, statuses)`：新增 host_id；同文件:138–142。
- `SyncStatusStore::mark_recent_completed(source, host_id, at)`：新增 host_id；同文件:263–268。
- `Store::reset_for_source(source, host_id)`：新增 host_id；`src/store/schema.rs:345`。
- `Store::mark_source_file_deleted(source, host_id, file_path)`：新增 host_id；`src/store/source_file.rs:364–369`。
- `CursorStore::load_file_cursors(source, host_id)`：新增 host_id；`src/store/cursor.rs:24–28`。
- `CursorStore::{load_opencode_cursor,load_zcode_cursor}(host_id)`：原无参数；同文件:76,121。
- `CursorStore::{save_opencode_cursor,save_zcode_cursor}(host_id, cursor)`：新增 host_id；同文件:112,161。

### 最小兼容替代的可行性

1. BASELINE_ROOT/src/domain/models.rs:8–36 的 SourceKind 为 9 个 variant，无 non_exhaustive；当前 `src/domain/models.rs:8–42` 为 11 个。把新来源作为绑定旧 variant 的 associated const 会合并相等性和 `as_str()` 身份。ADR 0015:20 与 ADR 0017:21 明确要求独立 `omp`、`antigravity_ide`。这种别名方案不满足当前语义。
2. BASELINE_ROOT/src/query/pricing.rs:13–20 只含 Static/Snapshot/Unpriced。ADR 0016:27–32 接受 SourceReported 与 source_cost；移除它们会影响真实来源成本、unpriced 查询和 recompute。
3. ADR 0014:33–65,76–96 接受 host、远端事件、host/source 认证。旧名 local-only、新名显式 host 的设计可能恢复个别方法，但不能解决旧公开 struct/enum 图。
4. 现在追加 non_exhaustive 仍会让旧穷尽 match/struct literal 失败；追加 private 字段同样破坏原公开完整构造式。
5. 恢复 static_v1 或 tui wrapper 可以修复个别 missing 项，完整门禁仍有上述阻断。不建议在尚未选择长期边界时先添加这些 wrapper。
6. 维持 1.x 在技术上可设计：冻结旧 SourceKind、UsageEvent、SyncShard、QueryFilter、snapshot/report、CLI enum 等相互引用的 API，建立独立新类型和转换。必须决定旧查询遇到新来源的表示方式；旧 Pi 同时扫描 Pi/OMP，新实现已经分源。该工作涉及 domain/query/store/parser/sync/commands，需要独立设计和完整兼容编译 fixture。现有证据不支持小范围恢复。

### 工具日志外的已知破坏

`subscription::fetch_all` 从异步返回 `UsageFetchReport` 改为 `UsageFetchOutcome { report, cache_hit }`：

- 原：BASELINE_ROOT/src/subscription/mod.rs:59–72。现：`src/subscription/mod.rs:66–85`。
- `.trellis/spec/llmusage/backend/tui-subscription-contracts.md:32–35` 明确记录 public breaking change，禁止 wrapper，等待独立 release approval。其“crate version stays 1.3.0”与当前 Cargo 1.4.0 已不一致，版本决策后需要主会话修正规范。
- 调用者需要报告时改为 `fetch_all(...).await.report`；需要缓存来源时保留 `cache_hit`。持久化 cache 仍存 UsageFetchReport（同契约:38–39），不要保存整个 wrapper。
- 该项没有出现在当前 11 类日志中。具体 lint 覆盖原因未查明。正式 pass 不能替代已知迁移清单和调用方验证。

### 可审阅的 2.0.0 准备方案

#### 保持的边界

- 保持原命令、v1.2.0 tag/SHA；不传 `--release-type major`，不抑制 lint。实际 package.version 表达已选择的 major。
- 保留独立来源 ID、host/source 作用域、远端 protocol/accounting 守卫、来源成本、ordinary sync 历史保留及显式 rebuild。
- crate major 不自动改变 SQLite schema、source accounting version 或远端 wire protocol。版本准备不运行真实 sync/rebuild/reset。
- 保留诊断任务当前对 ParseIssues、SourceSyncStats、SourceSyncStatus 的兼容实现；major 选择不扩大本次诊断 API 范围。

#### 真实版本入口：13 个文件

| 文件与入口 | 当前值 | 2.0 方案动作 |
| --- | --- | --- |
| `Cargo.toml:3` package.version | 1.4.0 | 根 crate 2.0.0 |
| `Cargo.lock:1593–1594` llmusage entry | 1.4.0 | 用 Cargo 更新本地 package entry |
| `desktop/src-tauri/Cargo.toml:3` llmusage-desktop | 1.4.0 | 桌面 crate 2.0.0 |
| `desktop/src-tauri/Cargo.lock:2799–2800,2837–2838` | 两个本地 package 为 1.4.0 | 同步 llmusage 与 llmusage-desktop |
| `desktop/src-tauri/tauri.conf.json:3` version | 1.4.0 | Tauri app 2.0.0 |
| `desktop/package.json:4` version | 1.4.0 | 桌面前端 package 2.0.0 |
| `desktop/package-lock.json:3,9` 顶层与 packages[空键] version | 1.4.0 | 同步两个产品 metadata 字段 |
| `README.md:9` | 1.4.0 | 当前 crate 版本行 |
| `README.zh-CN.md:9` | 1.4.0 | 当前 crate 版本行 |
| `docs/index.md:51` | 1.4.0 | 当前产品版本行 |
| `docs/zh/index.md:51` | 1.4.0 | 当前产品版本行 |
| `docs/reference/cli.md:3` | 1.4.0 | 当前命令文档版本行 |
| `docs/zh/reference/cli.md:3` | 1.4.0 | 当前命令文档版本行 |

`justfile:63–100` 的 `just version-sync <version>` 修改根 Cargo.toml、六份文档，再执行 `cargo update --offline --package llmusage` 更新根 Cargo.lock。覆盖 8 个文件，缺少 5 个 desktop 文件。desktop 是独立 crate；根命令不更新 desktop Cargo.lock。本研究只核实该缺口，不修改工具。获批后可用现有入口并精确补齐 desktop metadata，再由相应包管理器同步锁文件；不批量升级依赖或扩展发布工具。

不能全局替换 `1.4.0`：`Cargo.lock:899–900` 与 `desktop/src-tauri/Cargo.lock:1549–1550` 是第三方 finl_unicode；`desktop/package-lock.json:2310–2311` 是 expect-type。第三方版本和 checksum 保持不变。

`CHANGELOG.md:3–16` 的 1.4.0 历史保留，增加“2.0.0 / Unreleased”条目及中英文迁移入口，不把准备工作写成已发布。`.github/workflows/ci.yml:72–94` 当前只核对六份文档包含根版本，未完整核对 desktop metadata；实施验收必须直接检查上述 13 个文件的准确 package 字段。

#### 迁移说明内容

1. 接受 major 后重编译，补齐新增公开字段。仅在结构有 Default 且默认语义正确时使用 struct update；host、cursor durability、成本来源不得随意补值。
2. SourceKind match 增加 AntigravityIde/Omp；PricingStatus 增加 SourceReported；事件订阅者处理三个 RemoteHost variant。持久化/传输使用稳定 source ID，保留各来源身份。
3. ReportFilter 构造迁移为 `filter: QueryFilter { ... }`。host 查询区分明确 host 与无过滤。
4. 按前述 16 个方法迁移参数。操作本机使用现有 local host 常量，远端使用实际 host；不能把所有调用固定为 local。
5. unit PiParser 改为 PiFormatParser::pi()/omp()；需要扫描两类源时注册两个实例。
6. static_v1() 改为 embedded()。旧 `commands::tui::run(app)` 调用方采用当前 `commands::dash::run(app, false)`；CLI tui alias 的既有提示保持。
7. LossyRebuildRisk 调用方用借用或显式 Clone，保留 host 作用域。
8. fetch_all 使用报告/缓存来源的新返回结构。
9. 记录 enum 数值与派生顺序变化；需要稳定业务排序的下游重新验证。
10. 区分 crate major、schema、wire protocol、accounting version；沿用现有 SchemaTooNew/降级限制（CHANGELOG.md:17,55–56），不承诺旧二进制打开当前 schema。

#### 实施及验收顺序

1. 针对 2.0 边界与维护 1.x 的具体结果询问一次；独立诊断/时区工作继续。
2. 获批后主会话更新父子 PRD/design/implement 及陈旧规范。D4 分清“本次诊断未新增公开构造式破坏”和“整体版本边界正确声明并通过正式门禁”。
3. 同步 13 个版本入口，增加中英文迁移说明及未发布 Changelog；保留 dependency 版本、正式 baseline、数据版本。
4. 验证 root/desktop 本地 package、Tauri/npm metadata、六份文档一致；锁文件第三方版本/checksum 不变。
5. 验证 API integration target 及受影响 root Rust/desktop/docs；跨表面最终运行 `just ci`，按仓库要求使用 locked/all-features/单线程测试。
6. 实际运行原始 `cargo semver-checks --baseline-rev v1.2.0`，保存完整输出。预期工具按 1.2.0→2.0.0 的真实 major 接受已知 major 差异，只有实际输出才能证明通过。
7. **合法 major 边界下通过表示 API 差异与声明版本相符，不表示 v1.2.0 源码兼容恢复。** 行为测试和迁移清单仍独立验收，保留原始失败日志。
8. 真实 AC/门禁通过后继续 Antigravity preflight 和 writer profiling；提交、tag、release、安装保持各自授权边界。

## Files Found

- 当前任务 semver-check.log / semver-head-comparison.log：正式失败及增量对照证据。
- src/lib.rs / tests/api/facade.rs：公开 façade 与当前 API 编译用例。
- src/domain/models.rs、pricing.rs、pricing_catalog.rs：source/event/pricing 公开类型。
- src/query/filter.rs、reports.rs、snapshot.rs、top_sessions.rs、home_overview.rs：query/报告字段变化。
- src/parsers/mod.rs、pi.rs、driver.rs、file_state.rs：事件、parser 与 record/driver 公开结构。
- src/commands/mod.rs、report_args.rs、source_status.rs、dash.rs：CLI 类型、Store 参数与旧 tui 入口迁移。
- src/store/mod.rs、cursor.rs、source_file.rs、sync_status.rs、schema.rs：host 维度的 shard、cursor、inventory、risk、reset。
- src/subscription/mod.rs：工具日志外的异步返回类型破坏。
- docs/adr/0014–0017：remote、OMP、来源成本、Antigravity CLI/IDE 的已接受决策。
- justfile、.github/workflows/ci.yml、CHANGELOG.md 及上述 13 个版本文件：版本同步和正式 gate 入口。

## External References

本研究未联网查询。现有 cargo-semver-checks 0.50.0 日志指向下列官方资料：

- Cargo SemVer：`https://doc.rust-lang.org/cargo/reference/semver.html`。
- Rust struct expressions：`https://doc.rust-lang.org/reference/expressions/struct-expr.html`。
- non_exhaustive：`https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute`。
- 对应 lint：`https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.50.0/src/lints/`。

## Related Specs

- .trellis/spec/llmusage/backend/ci-toolchain-contracts.md：正式 baseline、版本批准、锁文件规则。
- .trellis/spec/llmusage/backend/tui-subscription-contracts.md：fetch_all 与 cache provenance。
- source-sync-contracts.md / token-accounting-contracts.md：来源认证、历史保留、显式 rebuild。
- docs/agents/domain.md、ADR 0014–0017：术语与语义范围。
- .trellis/workflow.md：研究持久化及实施/验证/提交边界。

## Caveats / Not Found

- 本研究没有修改代码、规范、任务状态、版本工具、锁文件或真实数据，没有执行 Git 操作、昂贵 semver/full gate；2.0 方案尚未实施或验证。
- 11 项是 lint 类别，不是全部受影响符号，也不是完整破坏清单；fetch_all 是已验证的日志外反例。
- 1.x 完整 façade 技术上可以设计，其新来源/host 投影及维护边界尚无获批方案。未发现满足当前全部语义的小范围兼容替代。
- 2.0 开发树准备不等于批准发布、创建 tag 或提前更新 release baseline。
- 正式日志行号早于部分诊断编辑；本报告优先给当前读取锚点。并行编辑可能移动行号，应按符号名定位。
- ADR 0015 自动迁移的历史叙述受后续 ordinary-sync 历史保留契约约束；本报告只采用独立 source/共享 parser 决策，不恢复自动重建。
- memory quick pass 仅恢复正式 gate 不可被 HEAD 对照替代的历史线索，已用两个实际日志和当前契约核实。
