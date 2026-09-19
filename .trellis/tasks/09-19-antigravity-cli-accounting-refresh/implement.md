# CLI execution

1. [x] E1 已取得独立 descriptor、13 原生脱敏 fixture 和全量整数 oracle；D2/D4 采用产品来源组与原子 staged repair（ADR0017）。
2. [x] 用户已授权/start，字段/steps/retry/identity 已实施并通过独立原生对账与回归；不要求旧错误总量相等。
3. [x] D3 WAL、快照、group replay；D4 版本/修复/remote，失败/取消/bounded 和 hook history 保护验证通过。
4. [x] README 双语、ADR0017、source/token specs 与样本证据同步。
5. [x] 针对 tests、跨面 just ci 各门禁和独立 review 完成；AC1–AC4通过，证据见父 research/implementation-validation.md。未 commit/push/archive。

AC1→D1/parser oracle；AC2→D2/parser+query；AC3→D2/D3/sync+WAL；AC4→D4/sync+store+remote。
先 cargo test --locked --all-features --lib parsers::antigravity -- --test-threads=1；再 --test sync antigravity 与相关 query/store/remote tests；最终 just ci。测试不碰生产 usage DB。共享 decoder 归 CLI child，IDE child 后接入；共享 docs 串行合并。
