# 2026-10-08 sync 输出与本机状态

只读。未运行 rebuild，未修改 `~/.llmusage/llmusage.db` 或 Antigravity 文件。

## 命令与版本

用户粘贴的是普通 `llmusage sync`。`PATH` 上的二进制是 `C:\Users\lyh\.cargo\bin\llmusage.exe`，自报 1.3.0，文件时间 2026-09-19。当前仓库 `Cargo.toml` 版本是 2.0.0。

表前同时出现 tracing `WARN` 和这条 stderr 文案：

`legacy token accounting; existing history was kept and this source was skipped for this round.`

警告的 `sources` 只有 `antigravity`。`antigravity_ide` 的 `↳` 和 `malformed=1` 出现在来源行与下一行之间。样本被压缩成 `Antigravitytrackeddatabasemissinghistorypreservedrestoreitorusee`。

## 数据库

| 来源 | 记账版本 | 事件 | 最新事件 | 文件状态 |
| --- | --- | --- | --- | --- |
| antigravity | 2，当前要求 3 | 3477，无未归属行 | 2026-08-31 | live 104 个 `.db`；missing 状态但文件仍在的 JSON 112 个 |
| antigravity_ide | 3 | 9591，无未归属行 | 2026-09-19 11:08 | 清单 501；现存 354；缺失 147 |

CLI 上次看到 `.db` 是 2026-09-02。IDE 清单的 `last_seen_at` 是 2026-09-19 12:29，缺失行当时仍被记成 `live`。

## 文件系统

`GEMINI_CLI_HOME` 未设置。

- `~/.gemini/antigravity-cli/conversations/`：114 个 `.db`。与清单按文件名重合 104 个，10 个尚未跟踪。
- `~/.gemini/tmp/**/chats/*.json`：112 个已跟踪路径都还在，父目录不是 conversations，当前发现规则不收录。
- `~/.gemini/antigravity-ide/conversations/`：500 个 `.db`。与 501 个跟踪文件名重合 354 个；磁盘新增 146 个；跟踪文件名缺失 147 个。
- 库存里的 IDE 路径带 Windows `\\?\` 前缀。1.3.0 和当前代码都会先 `canonicalize` 再比较，所以上面的前缀本身不能解释阻断。本轮阻断由 147 个已不存在的跟踪库造成；354 个仍在的库会被读到，但整组不能提交。

## 代码边界

- 普通 sync 在 `src/sync/engine.rs` 发现旧记账后，从写入集合移除该来源，并立即发 stderr 警告。
- 1.3.0 的 `record_failure` 把「tracked database missing」同时写入 `last_error` 和 malformed 样本。2.0.0 改成 `SourceIssueCode::TrackedMemberMissing`，`cli_line` 含 count、时间和动作。
- 2.0.0 在全部选中产品都已失败时，于打开 usage 表前返回。部分失败仍继续解码其他产品。
- 人读表由 `format_summary_lines_with_basenames` 打印到 stdout。`last_error` 和 parse issues 目前插在对应来源行之后。进度和 stderr 警告走另一个流，所以长警告出现在表前。
- 有损重建已有 `--allow-lossy-rebuild`。仓库没有交互提问库。非 TTY、重定向和 `--json-events` 不能阻塞在 stdin。
