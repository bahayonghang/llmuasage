# 实施顺序

1. 先改 `.trellis/spec/llmusage/backend/token-accounting-contracts.md`，再改 engine。例外只写成 `design.md` 的那一句：无窗口、三个流都是终端、非 JSON 的人读 sync 里，用户接受的 Antigravity CLI/IDE 才重建。错误矩阵、Good/Bad 和测试要求一起改。未同意、有窗口、非交互、serve 和其他来源的原句保留。
2. 给摘要格式器加表后 notice。先改测试，证明表头到 `TOTAL` 之间没有诊断行，warning/error 和样本在表后，窄宽与无 ANSI 仍成立。
3. 抽出 Antigravity 只读覆盖，供 family 阻断分类和提示共用。该函数不解码、不 fingerprint、不写库。family 在全部阻断时解码次数为 0，源级保护不增加 malformed；`FILES` 使用按根归属的发现数，changed、bytes、write 保持 0。
4. 普通 sync 在 legacy 移出写入集合之后、driver 之前调用只读覆盖。数量不读取旧 `source_issues`。补两类夹具：没有旧诊断，以及旧诊断和磁盘不一致。
5. 人读命令只在 stdin、stdout、stderr 都是终端，且没有 `--json-events`、`--rebuild`、`--recent-days` 时询问。问题在 stderr。接受的产品在其他来源完成普通同步后，走现有 family 有损重建，不调用 `reset_sources_for_rebuild`。覆盖只接受一个产品、回车全保留、重建失败后事件和 marker 一起保留，以及 Codex 等来源没有选择。
6. 生产入口测试覆盖 stdout 单独重定向、stderr 单独重定向、stdin 管道和 `--json-events`。管道里放入选择文本，历史仍然保留。另用一个一读就失败的 stdin，证明不允许询问时不会调用它。只注入选择回调不算完成这一步。
7. 带 `--recent-days` 时不询问、不重建。交互表后提示去掉窗口再运行；非交互表后给出不带窗口的显式有损命令。
8. 去掉人读进度里重复的长 legacy 段落。`--json-events` 的 stdout 仍是每行一个 JSON 事件。
9. 更新 `source-sync-contracts.md` 的人读摘要和询问条件，使它和上面的行为一致。
10. 按最终行为更新用户文档。只改仍在描述普通 sync、legacy 修复或 Antigravity 有损重建的段落：
    - `README.md`
    - `README.zh-CN.md`
    - `docs/guide/first-sync.md`
    - `docs/zh/guide/first-sync.md`
    - `docs/reference/cli.md`
    - `docs/zh/reference/cli.md`
    - `docs/safety/index.md`
    - `docs/zh/safety/index.md`
    - `AGENTS.md` 里那一句普通 sync 的修复入口
    `docs/safety` 若仍写着普通 sync 或 serve 会自动重建安全 legacy，改成现行契约加上这次交互例外，不顺手重写无关安全章节。
11. 最后跑 `just ci`。

## 验证

每一条都必须实际跑到测试。过滤参数没有命中任何测试时不算通过。

- `cargo test --locked --all-features --lib commands::sync_summary -- --test-threads=1`
- `cargo test --locked --all-features --lib parsers::antigravity -- --test-threads=1`
- `cargo test --locked --all-features --lib commands::sync:: -- --test-threads=1`
- `cargo test --locked --all-features --test sync accounting -- --test-threads=1`
- `cargo test --locked --all-features --test sync sources::antigravity -- --test-threads=1`
- 最后跑 `just ci`

失败时停在该步，不改 token 语义来通过测试。局部命令不能代替最后的 `just ci`。

## 不在本实施里做

不并行化 Antigravity 解码，不取消 family 现有的阻断前 fingerprint，不自动重建无损旧记账，不给非 Antigravity 来源做新的失败回滚，不对本机 `~/.llmusage` 执行 sync 或 rebuild，不安装或发布。
