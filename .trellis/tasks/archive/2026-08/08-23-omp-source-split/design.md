# 设计：拆出 omp 独立源

## 边界

改动集中在源注册、发现、解析器实例化与迁移触发；记录级解析规则不变。
三条迁移路径的共用决定写在父任务 `design.md` D1，本文件只写本子任务的落地方式。

## 决定 1：独立 SourceKind，而不是源内变体标签

采用 `SourceKind::Omp`。

- 被否方案 A：保持 `source='pi'`，新增一个「变体」列区分根。否决理由：
  `usage_bucket_30m` 的主键是 `(source, provider_label, model, hour_start, project_hash)`
  （ADR 0010），再加一列要改主键并迁移全部聚合行，改动面比新增枚举值更大；
  且 CLI `--source`、source-status、看板筛选都要额外识别一个新维度。
- 被否方案 B：照搬 ccusage 的配置化 named store。否决理由：llmusage 的源集合是编译期
  枚举 + 描述符表（`src/domain/source_descriptor.rs`），没有运行期动态源的通路。
- 代价：`SourceKind` 的每个 match 分支都要补 `Omp`，且若干硬编码字符串清单不会被
  编译器发现，见「需要改动的接线点」。

## 决定 2：按路径判定归属，双向

`src/parsers/source_files.rs` 拆成两个函数：

- `list_pi_session_files()`：根为 `PI_AGENT_DIR`（逗号分隔列表）或 `~/.pi/agent/sessions`，
  不再追加 `.omp` 根。
- `list_omp_session_files()`：根为 `~/.omp/agent/sessions`。不引入新环境变量
  （真源与 tokscale 都没有 OMP 专属环境变量，不自造配置面）。

归属唯一性按**路径**判定，而不是按根：`list_omp_session_files()` 先把 pi 侧的根
canonical 化，再对每个 omp 候选文件的 canonical 路径做前缀判定，命中任一 pi 根则跳过
该文件。

理由：`PI_AGENT_DIR` 可以是任意目录列表且发现是递归的（`WalkDir`，
`src/parsers/source_files.rs:258`），所以两个方向的嵌套都可能出现：

- `PI_AGENT_DIR=~/.omp/agent/sessions` → 两根相等；
- `PI_AGENT_DIR=~/.omp/agent/sessions/<project>` → pi 根在 omp 根**内部**。

只做「omp 根等于或位于 pi 根子树」的单向判定会漏掉第二种形态。路径级判定同时解决
两种形态，并且不会因为一个子目录配置而丢掉整个 `.omp` 根的其他项目——这一点优于
ccusage 的整店拒绝（`paths_overlap` + 报错，
`ref/repo/ccusage/rust/crates/ccusage-adapter-all/src/loader.rs:511`）。

方向选择 `pi` 优先，因为 `PI_AGENT_DIR` 是用户显式配置。被跳过的文件数记入 listing 的
`errors` 摘要，让 source-status 能解释差异。

## 决定 3：解析实现共用

`PiParser` 改为持有 `SourceKind` 与发现函数：

```rust
pub struct PiFormatParser {
    source: SourceKind,                       // Pi | Omp
    list_files: fn() -> SourceFileListing,
}
```

`SourceParser::source()` 返回 `self.source`，`sync_pi` 内所有 `SourceKind::Pi`
字面量改用 `self.source`，`event_key` 前缀改为 `format!("{}:{}", self.source.as_str(), hash)`。
`src/registry.rs` 注册两个实例。日志与进度事件里的源名同样取自 `self.source`。

`event_key` 前缀随源变化是有意的：它让存量 `pi:` 行与新的 `omp:` 行不会被
`INSERT OR IGNORE` 合并，从而使双计问题必须由迁移显式解决，而不是被静默掩盖。

## 决定 4：默认 sync 的迁移

`src/store/schema.rs` 的 `expected_token_accounting_version` 给 `Pi` 单独分支并把版本
从 `TOKEN_ACCOUNTING_VERSION`（2）提到 3，`Omp` 使用 `TOKEN_ACCOUNTING_VERSION`。
链条与守护条件见父任务 `design.md` D1.1。

被否方案：用 SQL 迁移把存量 `pi` 行改写为 `omp`。否决理由：需要同时改写 `event_key`
前缀与归属判定，而路径以哈希存储，迁移时只能靠枚举当前磁盘上的 `.omp` 文件反推哈希；
已删除的文件对应的行无法判定归属，会留下既非 pi 也非 omp 的残行。

## 决定 5：限定源同步的门

按父任务 D1.2，在 `src/commands/sync.rs` 的 `parsers` 过滤之后、流水线之前插入：

```text
if parser_sources 含 Omp 且不含 Pi 且 store.has_legacy_token_accounting(Pi)? {
    bail!(... 先执行一次不带 --source 的完整 `llmusage sync` ...)
}
```

这道门与 `--recent-days` 的既有拒绝分支（`src/commands/sync.rs:938`）风格一致。
注意 `has_legacy_token_accounting` 的行数统计不分 host（`src/store/schema.rs:196`），
所以只要任何 host 还有拆分前的 `pi` 行，这道门就会生效；这正是需要的行为。

## 决定 6：远端 host 级迁移

按父任务 D1.3：`meta` 增加 `omp_split_migrated.<host_id>`，远端导入路径在提交某 host 的
`Omp` shard 之前，若该 host 无标记则执行 `reset_for_source(Pi, host_id)` 并写标记。

实现位置在 `src/remote/importer.rs` 的 shard 绑定/提交处（`bind_shard_to_host` 附近，
`src/remote/importer.rs:132`）。标记必须在同一个写事务里落库，避免中断后重复重置。

`llmusage remote remove --delete-usage` 的源清单（`src/commands/remote.rs:134`）要补
`SourceKind::Omp`，否则删除 host 时会漏掉 `omp` 行。

## 需要改动的接线点

编译器能发现的（`cargo check` 的非穷尽 match 报错即清单）：

| 文件 | 改动 |
| --- | --- |
| `src/domain/models.rs` | `SourceKind::Omp` 枚举值、`as_str`、clap `value(name="omp")` |
| `src/domain/source_descriptor.rs` | 新增 `omp` 描述符（parser + passive_probe、Precise、LocalArtifacts） |
| `src/store/schema.rs` | `expected_token_accounting_version` 补 `Omp`、`Pi` 提版本 |
| `src/tui/report_table.rs` | 源配色补 `Omp` |
| `src/parsers/mod.rs` | 导出、`bounded_contract_parse` 的源映射补 `Omp` |

编译器**不会**发现的（必须靠全仓搜索源清单字符串）：

| 文件 | 改动 |
| --- | --- |
| `src/commands/help.rs:413` | `--source` 说明里的源清单字符串 |
| `src/commands/diagnostics.rs:132` | 找不到源时的错误提示清单 |
| `src/commands/remote.rs:134` | `remote remove --delete-usage` 的源枚举数组 |
| `src/web/assets/components.css` | 源色块（按记忆约定用 Bash 改，不用 Edit） |
| `docs/reference/cli.md:34`、`docs/dashboard/index.md:70` | 源清单表格 |
| `README.md`、`README.zh-CN.md`、`docs/index.md`、`docs/zh/index.md`、`docs/guide/first-sync.md`、`docs/architecture/index.md`、`docs/agents/passive-source-candidates.md` | 源清单与首同步说明 |

其余：`src/parsers/source_files.rs`（发现拆分）、`src/parsers/pi.rs`（参数化）、
`src/registry.rs`（双实例）、`src/domain/platform_monitor.rs`（`pi` 去掉 `.omp` 根、
新增 `omp` 描述符）、`src/remote/importer.rs`（host 迁移）、
`src/commands/sync.rs`（限定源门）、`tests/sync_regression.rs`、
`tests/remote_lifecycle.rs`、`docs/adr/0015-omp-source-split.md`。

## 兼容性

- schema 无结构变化（只增 `meta` 键），`schema_version` 不需要提升。
- `SourceKind::parse_id("omp")` 在旧二进制返回 `None`；实现时验证
  `src/domain/source_descriptor.rs` 的 `parse_source_id` 调用点不会 panic。
- 回滚步骤见父任务 `design.md` D4。
