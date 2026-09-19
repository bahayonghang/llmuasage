# IDE execution

1. [x] 用户授权独立IDE来源并start；原生fixture及source17CLI/1IDE证明归属，旧根/backup排除；E1/E2收敛见ADR0017。
2. [x] 已接入native root与共享decoder，发现/多身份/跨根/CLI共存fixture回归通过。
3. [x] D3 WAL/read-only/失败/取消/bounded/history/remote/rebuild；D4新增source所有消费者与status传播验证通过。
4. [x] 双语docs/ADR/spec/candidates同步；Windows530DB完整只读导入独立对账，研究输出只含脱敏metadata和整数。
5. [x] targeted gate、跨面just ci各门禁、独立review完成；AC1–AC4通过，证据见父research/implementation-validation.md。未commit/push/archive。

AC1→D1/shared oracle；AC2→D2/sync混合样本；AC3→D3/sync+store+remote+Windows live；AC4→D4/cli+api+query+tui+JS+desktop+docs。
先 cargo test --locked --all-features --lib antigravity -- --test-threads=1 与 --test sync antigravity；按影响扩展现有八targets，最终just ci。共享decoder归CLI child，IDE接入排在其合同/实现之后；无RPC或新依赖获授权。
