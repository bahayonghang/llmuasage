# 时区门禁修复记录

## 范围与结果

用户已授权先扩大范围修复既有门禁。该实现仅修改 `src/query/filter.rs` 与 `src/web/mod.rs` 的时区注释和测试。产品查询、HTTP 默认时区、日期分组、SQL 参数生成与持久化逻辑没有修改。父任务负责规范和双语文档。

相关定向测试共 30 项通过，0 项失败，886 项被过滤。正式根 Rust、跨表面与 semver 门禁仍由主代理统一执行。

## 契约与原因

### 1. Local 测试保留了失效的固定偏移假设

`.trellis/tasks/archive/2026-07/07-24-dst-timezone/prd.md:5,13-16` 的 DATA-003 要求 `ReportTimezone::Local` 按目标日期使用历史 IANA 偏移，并更新锁定旧行为的测试和文档。当前实现 `src/query/timezone.rs:38-47` 先读取系统时区名，再解析为 `chrono_tz::Tz`；成功时使用 IANA 规则。系统名称读取或解析失败时，回退到当前本机固定偏移（`src/query/timezone.rs:128-130`）。

旧 `local_timezone_date_bounds_use_current_fixed_offset_snapshot` 测试把 `Local::now().offset()` 同时应用到开始和结束日期，与 DATA-003 冲突。在 America/Chicago，2026-11-01 的正确 UTC 范围为 `[2026-11-01T05:00:00Z, 2026-11-02T06:00:00Z)`，跨度 25 小时。旧测试在夏令时运行时把结束边界错误地设为 `05:00:00Z`。

修复后的 `local_timezone_date_bounds_use_system_zone_rules`（`src/query/filter.rs:292`）通过系统名称和 Chrono 时区换算独立建立预期，不调用被测的 `ResolvedZone::local_date_start_utc`。系统名称不可解析时，预期保持既有固定偏移回退。`ReportTimezone::Local` 的公开注释同步到实际契约（`src/query/filter.rs:12`）。

新增 `iana_timezone_date_bounds_follow_short_and_long_dst_days`（`src/query/filter.rs:338`）直接检验 `QueryFilter::event_filter` 的 SQL 运算符与两个绑定边界。测试使用显式 `America/Chicago` 和固定预期值，独立于运行机器的时区：

| 本地日期 | UTC 下界（包含） | UTC 上界（不包含） | 小时数 |
| --- | --- | --- | ---: |
| 2026-03-08 | 2026-03-08T06:00:00Z | 2026-03-09T05:00:00Z | 23 |
| 2026-11-01 | 2026-11-01T05:00:00Z | 2026-11-02T06:00:00Z | 25 |

### 2. 两个 Web 测试把默认 Local 当作 UTC

`query_timezone` 对省略时区的 HTTP 请求返回 `ReportTimezone::Local`（`src/web/mod.rs:2081`）；既有解析测试保留该默认行为（`src/web/mod.rs:2294`）。

`api_trends_daily_exposes_daily_cost_series` 的固定种子为 `2026-05-01T00:00:00Z`，断言日期为 `2026-05-01`，但请求未指定时区。在 America/Chicago，该 instant 的日期为 `2026-04-30`。修复请求为 `/api/trends_daily?timezone=UTC`（`src/web/mod.rs:5329`），保留全部 token、event count 与 cost 断言。

`behavior_apis_return_activity_tools_and_snapshot_fields` 的普通请求已有 `timezone=UTC`，但单独的 `day_two_filter` 遗漏该参数。其 non-tool event 为 `2026-05-02T01:00:00Z`，在 America/Chicago 属于 5 月 1 日，因此按本地 5 月 2 日过滤返回 0 行。为该过滤串补上 `timezone=UTC`（`src/web/mod.rs:5700-5701`），保留 1 行、`(non-tool)` 和 USD 0.25 的原断言。

## 验证记录

当前 Windows 系统报告 `Central Standard Time`，基础 UTC 偏移为 `-06:00:00`。测试未修改进程 `TZ` 或系统时区。

### 初次原测试运行

```text
cargo test --locked --all-features --lib -- query::filter::tests::local_timezone_date_bounds_use_current_fixed_offset_snapshot web::tests::api_trends_daily_exposes_daily_cost_series web::tests::behavior_apis_return_activity_tools_and_snapshot_fields --test-threads=1
```

工具观察到 exit 1，测试未运行。构建失败来自并行诊断实现的 `src/parsers/antigravity/decode.rs:52`：`thiserror` 将 `source: SourceKind` 推导为错误来源，报 `E0599 as_dyn_error`。诊断检查代理随后将该字段改为 `source_kind`，并报告 `cargo check --locked --all-features` exit 0。本报告不把该构建失败计为三个时区用例的重新复现；原三项失败来自父会话已记录的完整 lib 运行。

### 修复后定向验证

```text
cargo test --locked --all-features --lib -- query::filter::tests web::tests::api_trends_daily_exposes_daily_cost_series web::tests::api_trends_daily_groups_by_iana_timezone_with_dst_rules web::tests::behavior_apis_return_activity_tools_and_snapshot_fields web::tests::query_timezone_accepts_iana_and_preserves_legacy_fallbacks query::timezone::tests --test-threads=1
```

观察结果：exit 0；`cargo test: 30 passed, 886 filtered out (1 suite, 0.38s)`。覆盖修复后的三项测试、新增 23/25 小时边界、既有 Fixed/UTC 行为、IANA HTTP 分组、HTTP 默认/未知时区回退和 `query::timezone` 单元测试。

```text
git diff --check -- src/query/filter.rs src/web/mod.rs
rustfmt --edition 2024 --check --config skip_children=true src/query/filter.rs src/web/mod.rs
```

检查通过，命令组 exit 0。Git 仅报告工作副本 LF/CRLF 提示；未报告空白错误。

## 验证边界

- 没有更改公共 Rust 类型、HTTP 默认值、token/cost 算法或数据库。
- 没有读取或重放真实用户 usage 数据，没有执行安装、提交、sync、rebuild 或 reset。
- 本机验证覆盖实际系统时区解析路径；没有人为制造系统时区发现失败。显式 Fixed 回归继续通过。
- `just ci`、完整根 Rust 门禁及正式 semver 门禁尚不属于本实现代理的验证结果，须由主代理完成。
