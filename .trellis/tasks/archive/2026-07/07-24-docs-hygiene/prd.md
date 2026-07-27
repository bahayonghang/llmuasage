# 文档版本与仓库卫生（DOC-001、HYGIENE-001）

## Goal

消除版本文本漂移与仓库卫生问题：文档/注释版本引用统一，生成物移出 git 跟踪，命名拼写差异显式化。

## 覆盖发现（已核实）

- **DOC-001（P3）**：`Cargo.toml` 版本 1.0.2；`src/lib.rs:7` 注释仍称 "0.7.x compatibility"；`docs/architecture/index.md:3,70` 称 "0.6.x"；`.github/workflows/ci.yml:40-65` version check 只覆盖有限文件，未能拦截上述漂移。
- **HYGIENE-001（P3）**：package/binary 名 `llmusage`，GitHub repo 名 `llmuasage`（多一个 a）；`.gitignore:35` 已忽略 `output/`，但 `output/playwright/*.png`、`sync-command-center-verify.json` 等产物仍被 git 跟踪（ignore 不会自动移除已跟踪文件）。

## Requirements

1. 文档与注释改用"current"描述或生成版本页，不硬编码历史版本号；本次先把 0.6.x/0.7.x 引用改正确。
2. CI version check 扩展为搜索旧版本 pattern（如 `0\.[0-9]+\.x`）而非仅比对固定文件清单。
3. `git rm --cached output/` 移除已跟踪生成物；Playwright 证据改走 CI artifact 上传或任务目录归档。
4. repo 名 vs crate 名：能迁仓则 rename；否则在 README 顶部明确 canonical naming，避免链接/搜索错拼（需 owner 决策，本任务默认做文档显式化）。

## Acceptance Criteria

- [ ] 全仓搜索无 0.6.x/0.7.x 之类过期版本引用（CI pattern gate）。
- [ ] `git ls-files output/` 为空；后续构建不再产生 output/ 下的 diff 噪音。
- [ ] README（中英文）含 canonical naming 说明或 repo 已 rename。
- [ ] `just ci` 全绿（docs build 含在内）。

## Notes

- 轻量任务：PRD-only 即可启动。
- 注意仓库规约：不得提交本地用量数据；移除 output/ 时确认无个人数据混入历史（如需彻底清除历史需另行评估，不在本任务范围）。
