# Antigravity 任务实施计划

- [x] diagnostics-contracts 完成后，加载父研究、ADR 0017 和三个相关 backend 契约。
- [x] 先添加全选中产品阻断仍触发解码的失败测试，外加旧 JSON/out-of-scope 路径分类反例。
- [x] 仅实现安全全阻断预检返回，记录所选产品 code/数量/观察时间；保持现有 partial 路径。
- [x] 核对 stats 与持久诊断，测试没有 usage 写入、cursor 推进和 marker 认证。
- [x] 验证恢复、复制归属、强身份、WAL、取消、bounded、Windows 路径与 rebuild 权限矩阵。
- [x] 更新 source-sync 契约与双语用户文档，独立检查 A1–A5。

~~~text
cargo test --locked --all-features --test sync antigravity -- --test-threads=1
cargo test --locked --all-features --test store -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
just ci
~~~

以 tests/sync/sources/antigravity.rs 的缺成员、copied root、WAL、busy、取消、Windows path 用例为基础。新测试只用临时目录和隔离库。禁止在真实目录删除147个缺失引用、补造 DB 或运行 rebuild。若新的源级 JSON 契约仍不足以表达观察结果，先同步父任务，不另加状态表。

2026-09-28：独立检查修复 discovery-time root 错误丢失；只将 NotFound 视作空根。source_files/preflight 17项通过；完整 just ci exit 0，251.635秒，lib 915、集成247，4 lock字节不变。A1–A5完成；提交/归档待统一确认。

## 提交与归档交接（2026-09-28）

用户已确认交付方案。工作提交 `cc7eed3`、`6389ceb` 已完成，产品文件字节保持不变。验收及门禁沿用已通过证据；本轮不重复测试。任务随后使用 Trellis 归档脚本标记 completed 并归档，context 路径已同步到同月归档位置。
