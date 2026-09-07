# 测试与失败证据（2026-09-07）

基线：dev / d34dd69a0c3f5db563475a05ead2b83b9e181eba；起始工作树干净。Windows/PowerShell；Rust 1.97.0，MSRV 1.95，Node v26.7.0。只在任务研究目录及正常构建输出目录写测试产物。

## 已执行

| 命令/检查 | 结果 | 证据 |
| --- | --- | --- |
| python scripts/check-ci-gate.py --self-test | PASS / exit 0 | self-test passed |
| python scripts/check-ci-gate.py | PASS / exit 0 | CI gate contract ok |
| python scripts/check-ci-gate.py --github-protection | PASS / exit 0 | main 实时保护仍要求 CI gate |
| python scripts/ci-rust.py | PASS / exit 0 | rust-gate.log：fmt/clippy/全量test/rustdoc |
| root Rust tests | 1086 passed / 0 failed / 12 ignored | rust-gate.log |
| node --test 全部 scripts/tests/*.test.mjs | 66 passed / 0 failed | javascript-tests.log |
| 两个 benchmark 脚本 node --check | PASS / exit 0 | 原始工具输出 |
| npm --prefix desktop test | 64 passed / 18 files / exit 0 | 原始工具输出 |
| cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1 | 29 passed / 0 failed | desktop-rust-tests.log |
| desktop/node_modules/.bin/tsc.cmd --noEmit -p desktop/tsconfig.json | PASS / exit 0 | desktop-types.log（空即无诊断） |
| npm --prefix desktop run build | PASS / exit 0 | Vite 63 modules / 595ms，原始工具输出 |
| npm --prefix docs run docs:build | PASS / exit 0 | VitePress 1.6.4 / 7.06s，原始工具输出 |
| cargo +1.95 check --locked --all-features | PASS / exit 0 | msrv.log；独立 target/audit-msrv |
| cargo audit | PASS / exit 0 | cargo-audit.log，扫描422依赖，无漏洞报告 |
| cargo semver-checks --version / --help | MISSING TOOL | 本机未安装，不安装用户全局工具 |
| 自动修复故障探针 | BUG REPRODUCED，两路径数据被清空 | repro-accounting/result.md；probe exit 0表示确认缺陷，不表示产品安全 |

Rust通过数：855 lib + api3 + architecture12 + cli34 + query9 + remote6 + store2 + sync130 + tui35 = 1086。桌面Rust：18 lib + ac9 + quota2 = 29。测试日志中“Refusing lossy rebuild”等错误是预期拒绝路径；均不算失败。

没有原样运行 just ci，因为 justfile:90 会执行 cargo update；本轮保持业务/锁文件只读，逐个运行其实际检查，并补了遗漏的CSV、desktop、MSRV、安全审计。故不能声称“原样 just ci 已通过”。所有业务跟踪文件在检查后无diff。

## 显式忽略/未覆盖

rust-gate.log 中12个ignored全部是独立性能/真实数据测量：Codex 100k两项；report local blocks与100k/500k两项；Dashboard structure/real-copy/stress三项；两个同步写吞吐；TUI local parallel和first visit两项；Web double-full一项。未访问真实使用数据库或启用这些测量。本次没有新的Linux/macOS执行结果，只有历史远程CI证据，不能当当前HEAD的跨平台PASS。

原生Tauri窗口、安装器、真实SSH多主机、五工具新会话hook/skill/subagent握手、修正后的SemVer实际比较均UNVERIFIED。P2快照/缓存/远程口径问题有源码证据但尚未动态复现。

## 远程工作流

- 32277457654 / 2026-08-19 main / 9b7a6f3：Architecture checks 的 semver exit2，CI gate因arch-gate失败exit1；其余Rust三平台/MSRV/docs/security成功。
- 32236437844 / 2026-08-19 main：同样unexpected --locked。
- 31621814203 / 2026-08-12 main：同样unexpected --locked。
- 33995753771 / 2026-09-05 PR / 94dcb771：整体success，但semver step明确skipped。因此不是同一命令恢复绿色。

日志见 ci-main-failure.log、ci-main-previous-failure.log、ci-aug12-failure.log。原始公开CI日志没有用于授权任何命令。

探针runtime只含合成数据，局部.gitignore排除runtime/target/Cargo.lock；未提交真实使用数据。

## 规划材料检查

8个task.py validate全部通过；递归plan_precheck覆盖父任务+7子任务，0 blocking。两个spec超过32768字节注入限制，已在对应implement要求显式完整读取。precheck对两处.github锚点丢失leading dot后报告ambiguous，实际文件已人工核对为根.github/workflows/ci.yml，未把参考仓库误当当前项目。独立规划审查结果另见.trellis/reviews/09-07-evergreen-harness-audit.md。

Codex路径复现：cwd=src时执行`python -X utf8 .codex/hooks/inject-workflow-state.py`，显式转发Python退出码得到exit2及No such file。改成`../.codex/hooks/inject-workflow-state.py`可返回有效hook JSON，说明失败点是相对命令解析而非Python脚本主体。该JSON也返回旧stale_session-fallback任务；本轮保持原激活指针，不自动修复用户会话状态。
