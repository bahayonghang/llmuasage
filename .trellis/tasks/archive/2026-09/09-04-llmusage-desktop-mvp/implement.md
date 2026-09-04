# llmusage Desktop MVP — Implement

父任务不改产品代码。批准规划后 `task.py start` **第一个子任务** `desktop-shell-ipc`。不要 start 父任务。

子任务 2–5 已各有 `design.md` / `implement.md`。顺序与依赖以本文件与各子 PRD 为准：windows-bundle 在 shell、core、secondary、ops **均完成**后才打最终包。

## Ordered checklist

1. **desktop-shell-ipc**
   - 目标：`desktop/` + `desktop/src-tauri`，DTO、supervisor、bootstrap、command 表。
   - 关键符号：`FilterDto`、`convert_filter`、`DesktopQuerySupervisor`、`AppContext::discover`。
   - 最小验证：`cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1`

2. **desktop-core-ui**（依赖 1）
   - 目标：React 壳、筛选、核心快照、运行状态、sync 控件映射、视觉 token。
   - 关键符号：`runtime/invoke.ts`、`loadDashboardProgressive`、`syncOptionsFromState`。
   - 最小验证：`tauri dev` 核心块 + 筛选连切；hosts≤1 隐藏。

3. **desktop-secondary-ui**（依赖 2）
   - 目标：次级并发 2、`home_overview` 六卡、heatmap 钻取、explorer。
   - 关键符号：`SECONDARY_SECTIONS`、`runLoadersWithConcurrency`。
   - 最小验证：核心已画时单块失败不影响其它块。

4. **desktop-ops-quota**（依赖 2；可与 3 并行）
   - 目标：logs/CSV/prefs/刷新/额度。
   - 关键符号：`LogsDto`、`buildAnalyticsCsv`、`FetchContext`、`cache_hit`。
   - 最小验证：Desktop cargo test 额度本地监听 + CSV 文件头 BOM。

5. **desktop-windows-bundle**（依赖 1–4 全部完成）
   - 目标：NSIS、just、gitignore、文档。
   - 关键符号：`tauri.conf.json` bundle.targets、`just desktop-build`。
   - 最小验证：`python scripts/check-ci-gate.py`；本机 NSIS 目录有安装包。

## 树级集成门禁（TPR-05）

五个子任务的产品改动都落地后，回到**父任务**执行本门禁。这是任务树进入完成/归档前的唯一总关口。windows-bundle 的 `just desktop-build` 产出作为最终构建；若安装包不便自动化，允许同一提交的 `tauri dev` 构建用于功能项，并在记录里写明用了哪一种。

### 自动化

| 项 | 命令 | 覆盖 |
|---|---|---|
| 根门禁 | `just ci`（本轮动过根文件时） | AC4, AC23 |
| CI 契约 | `python scripts/check-ci-gate.py` | AC4 |
| Desktop Rust | `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1` | AC2, AC3, AC8, AC15, AC7 rebuild=false, AC13 本地额度 |
| Desktop 前端 | `npm --prefix desktop test` | 映射表、generation |
| 安装包存在 | 检查 `desktop/src-tauri/target/release/bundle/nsis/` | AC14 |

### 人工（最终 Windows 构建）

逐条执行 AC1、AC5–AC7、AC9–AC12、AC16–AC22、AC24–AC27。亮/暗 × zh/en × 1440/720 至少各一组截图或检查记录。

### 记录

在父任务 `notes` 或 workspace journal 列出：通过的 AC、失败的 AC、明确未验证项（macOS/Linux 编译、签名、SmartScreen）。**未验证不得记成通过**。

全部自动化项通过、人工项无失败、未验证项已列出之后，才允许对任务树执行完成/归档。本文件不授权现在运行 `task.py start`。

## Validation

| 步骤 | 命令 |
|---|---|
| 根门禁未破 | `just ci`（windows-bundle 或每子任务结束时若动了根文件） |
| 根 crate 切片 | 仅当改了 `src/`（interrupt_handle）：`python scripts/ci-rust.py` |
| Desktop Rust | `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1` |
| Desktop 前端 | `npm --prefix desktop test`（子任务引入测试后） |
| 额度 | Desktop 测试注入 `UsageEndpoints`；禁止公网 |
| 安装包 | Windows：`just desktop-build`，确认 `desktop/src-tauri/target/release/bundle/nsis/` |

## Risky files

- `justfile`、`.gitignore`、`.github/workflows/ci.yml`（禁止改 `CI gate` 的 `name:`）
- `src/lib.rs` 与 façade：允许的唯一产品补丁是 `Dashboard::interrupt_handle` 改为 `pub`
- `src/web/**`、`src/tui/**`：本任务不应改 serve/TUI 行为
- `~/.llmusage`：测试必须用 `Fixture` / 临时 `with_root` / `AppContext::with_cli_home`

## Rollback points

- 子任务 1 失败：删 `desktop/`，还原 `interrupt_handle` 可见性，根树应干净
- 子任务 2–4 失败：保留 IPC，回退对应 `desktop/src` feature
- 子任务 5 失败：不影响已能 `tauri dev` 的功能面

## Before `task.py start`

- 用户已批准本父任务最新规划摘要
- 五个子任务目录已创建且 PRD 指向父 R/AC
- 先 start `desktop-shell-ipc`，不要 start 父任务
- 本轮审阅修订尚未构成该批准
