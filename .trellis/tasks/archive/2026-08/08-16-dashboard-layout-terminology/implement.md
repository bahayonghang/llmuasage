# Implement：用量概览布局与术语优化

## Implementation Checklist

1. 在 `components.css` / `charts.css` 为 `.analytics-heatmaps` 增加宽屏双栏和中窄屏单列回退；让 `.hour-week-svg` 响应式使用分区宽度，并仅对长范围日历启用等比缩放。
2. 审计并更新 `UI_COPY_ZH` / `UI_COPY_EN`：概览摘要、热力图、会话排行、状态、同步中心、行为分析、用量分析与诊断文案采用统一概念。
3. 审计并更新 `SHELL_COPY_ZH` / `SHELL_COPY_EN` 与 `shell.rs` 默认值：导航、筛选、行为分析、用量分析、成本和运行区不再混用含糊术语。
4. 同步 `csv-export.js` 中的摘要与会话段名称。
5. 同步 `docs/dashboard/index.md` 与 `docs/zh/dashboard/index.md` 的部件名称和布局描述。
6. 增加布局和术语回归断言，先运行最小 Web/Node 测试，再运行完整质量门。
7. 用隔离 Dashboard fixture 做中文/英文、浅色/深色、宽屏/移动视觉验收并记录几何证据。

## Validation Commands

```bash
cargo test --lib web::tests::overview_wide_layout_avoids_orphan_blank_columns
node --test scripts/tests/dashboard-render-lifecycle.test.mjs scripts/tests/dashboard-csv-export.test.mjs
python scripts/ci-rust.py
npm --prefix docs run docs:build
just ci
```

## Risk and Rollback Points

- `copy.js` 的中英文对象结构必须继续一致；不新增或删除 key，避免运行时回退。
- `shell.rs` 与中文 map 必须同步，避免首次渲染闪现旧文案。
- 小时图的 `min-width` 只允许在 `.heatmap-scroll` 内产生局部滚动，不能放大为页面级溢出。
- 若双栏在实际主栏宽度下过窄，优先上调回退断点，不缩小 7×24 网格到不可读。

## Review Gates

- 检查最终 diff 只触及 Web 展示、测试、文档和 Trellis 任务文件。
- 搜索被替换术语，区分用户可见文案与内部字段/注释；内部契约名不得为追求文案一致而重命名。
- 视觉验收确认空态与有数据态都成立，不能只验证截图中的单日空态。
