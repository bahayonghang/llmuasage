# Desktop secondary UI — Implement

依赖 `09-04-desktop-core-ui`。

## Checklist

1. 次级 loaders 并发 2，generation 守卫。
   - 文件：`desktop/src/app/secondary.ts`
   - 符号：`SECONDARY_SECTIONS`、`runLoadersWithConcurrency`
   - 验证：单测并发上限 2；generation 丢弃

2. `home_overview` 六卡。
   - 文件：`desktop/src/features/overview/SummaryCards.tsx`
   - 符号：sessions/requests/tokens/cost/active_days/cache_efficiency
   - 验证：失败 fixture 下核心仍在、六卡 degraded

3. heatmap / hour-of-week / top-sessions / trends-daily。
   - 文件：`desktop/src/features/heatmap/*`、`sessions/*`、`trends/*`
   - 符号：日期 toggle；`TopSessionsDto.sort`
   - 验证：AC5 单测；session 跳转事件形状

4. behavior 四块。
   - 文件：`desktop/src/features/behavior/*`
   - 符号：`insufficient_models` / `degraded` / `unsupported` 渲染
   - 验证：AC4 fixture

5. explorer 控件。
   - 文件：`desktop/src/features/explorer/*`
   - 符号：`ExplorerDto` 全字段
   - 验证：改控件只 invoke `explorer`；unsupported 禁用

## Validate

核心已画时单块失败不影响其它块；explorer 改控件只重拉 explorer。`npm --prefix desktop test`。

## Rollback

删除 `desktop/src/features` 下次级面板，保留核心壳。
