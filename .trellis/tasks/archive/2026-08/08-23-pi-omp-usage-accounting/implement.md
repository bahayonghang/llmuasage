# 执行计划：集成与验收

本文件只覆盖跨子任务的集成动作。各子任务自己的步骤在其 `implement.md`。

## 前置：升级前必须留下的基线

在合并子任务 1 之后、执行第一次升级 `sync` **之前**：

1. [x] 备份数据库：复制 `~/.llmusage/llmusage.db`（约 1.1 GB，确认磁盘余量），
       或 `llmusage export`。
2. [x] 导出身份基线（AC3 用）：

```sql
-- 存到 baseline_pi_identity.csv
SELECT source_path_hash, event_at, model, total_tokens
FROM usage_event WHERE source='pi' ORDER BY 1,2,3,4;
```

3. [x] 记录聚合基线：

```sql
SELECT COUNT(*), SUM(total_tokens), SUM(cost_with_cache_usd)
FROM usage_event WHERE source='pi';
```

4. [x] 跑一次真源扫描并存档输出：
       `python .trellis/tasks/08-23-pi-omp-usage-accounting/research/scan_pi_source.py`

## 集成步骤

1. [x] 子任务 1 合并后执行 `llmusage sync`（不带 `--source`、不带 `--recent-days`），
       确认日志出现 `Pi` 的自动 token accounting 重建。
2. [x] 验证 AC3：把升级后 `omp` 行的 `(source_path_hash, event_at, model, total_tokens)`
       集合与基线做差集，基线侧差集必须为空。

```sql
-- 期望 0 行
SELECT b.* FROM baseline b
LEFT JOIN (SELECT source_path_hash, event_at, model, total_tokens
           FROM usage_event WHERE source='omp') o
  ON b.source_path_hash=o.source_path_hash AND b.event_at=o.event_at
 AND b.model=o.model AND b.total_tokens=o.total_tokens
WHERE o.source_path_hash IS NULL;
```

3. [x] 验证 AC4：`SELECT COUNT(*) FROM usage_event WHERE source='pi';` 为 0（本机），
       且不存在 `event_key LIKE 'pi:%'` 与 `omp:%` 指向同一 `(source_path_hash, event_at)`
       的成对行。
4. [x] 验证 AC5：另建一个临时 home 的库，构造拆分前的 `pi` 行，跑 `sync --source omp`，
       断言被拒绝且没有写入 `omp` 行。
5. [x] 验证 AC6：按 D1.3 的选择执行——实现 host 级迁移则跑远端集成测试；
       走文档兜底则确认文档与命令输出都写明手动步骤。
6. [x] 子任务 2、3、4 全部合并后执行 `llmusage sync --rebuild --source omp`（AC12）。
       四个子任务在首次升级 sync 前已全部落地，因此一次不带 `--source` 的
       `llmusage sync` 已按新解析写入 provider、项目、成本与行为事实。
7. [x] 重跑真源扫描，用当次输出验证 AC7–AC10：

```sql
SELECT COUNT(*), SUM(CASE WHEN provider_label='' THEN 1 ELSE 0 END),
       COUNT(DISTINCT provider_label),
       SUM(CASE WHEN project_hash IS NULL OR project_hash='' THEN 1 ELSE 0 END),
       ROUND(SUM(cost_with_cache_usd),6)
FROM usage_event WHERE source='omp';

SELECT ROUND(SUM(cost_with_cache_usd),6) FROM usage_bucket_30m WHERE source='omp';
SELECT COUNT(*) FROM usage_tool_call WHERE source='omp';
SELECT COUNT(*) FROM usage_turn WHERE source='omp' AND retries >= 1;
```

判定：`provider_label` 空数为 0；`project_hash` 空数为 0；事件成本与扫描
`cost_total` 差 < 1e-6；桶成本与事件成本差 < 1e-6；`usage_tool_call` 行数等于
扫描 `tool_call_blocks`。

8. [x] 验证 AC8 的嵌套布局：确认某个 `<project>/<run-dir>/<Name>.jsonl` 产生的行与
       同项目顶层文件产生的行 `project_hash` 相同。
9. [x] 验证 AC11 负向隐私断言：

```sql
-- 期望 0 行
SELECT COUNT(*) FROM usage_event WHERE COALESCE(session_label,'') LIKE '%agent/sessions%';
SELECT COUNT(*) FROM usage_tool_call WHERE COALESCE(safe_preview,'') LIKE '%agent/sessions%';
```

10. [x] 验证 AC13：`llmusage --help`、`llmusage daily --help`、
        `llmusage diagnostics` 的源错误提示、以及 `README*.md` 与 `docs/` 的源清单
        都出现 `omp`。
11. [x] `just ci`（AC1）。

## 已知的既有缺口（本任务不修，需在评审时确认处置）

`--source` 的硬编码源清单目前已经落后于代码：`src/commands/help.rs:413`、
`src/commands/diagnostics.rs:132`、`docs/reference/cli.md:34`、
`docs/dashboard/index.md:70` 只列到 `grok`，缺 `zcode` 与 `deepseek_harness`。
子任务 1 会把 `omp` 加进这些清单；是否顺手补 `zcode` / `deepseek_harness`
需要评审时决定，默认不改（超出本任务范围）。

## 回滚

按 `design.md` D4 的四步执行。触发条件：AC3 差集非空、AC4 出现成对行、
或 AC9 桶成本与事件成本不一致且无法在当次修复。
