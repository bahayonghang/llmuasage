use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::common::i18n::Language;

pub fn find_command<'a>(root: &'a clap::Command, path: &[String]) -> &'a clap::Command {
    path.iter().fold(root, |command, name| {
        command.find_subcommand(name).unwrap_or(command)
    })
}

/// Human help is projected from ArgId and command metadata, never from rendered help text.
pub fn command_help(root: &clap::Command, path: &[String], language: Language) -> String {
    if path.is_empty() {
        return top_level_help(match language {
            Language::English => HelpLanguage::English,
            Language::Chinese => HelpLanguage::Chinese,
        });
    }
    let command = find_command(root, path);
    let title = command_description(path, language).unwrap_or_else(|| command.get_name());
    let mut output = format!(
        "llmusage {} — {title}\n\n{} llmusage {}",
        path.join(" "),
        language.select("Usage:", "用法："),
        path.join(" ")
    );
    if command
        .get_arguments()
        .any(|arg| arg.get_long().is_some() || arg.get_short().is_some())
    {
        output.push_str(" [OPTIONS]");
    }
    for arg in command.get_positionals() {
        output.push_str(&format!(" {}", argument_syntax(arg)));
    }
    if command.get_subcommands().next().is_some() {
        output.push_str(" <COMMAND>");
    }
    output.push('\n');
    if command.get_subcommands().next().is_some() {
        output.push_str(language.select("\nCommands:\n", "\n子命令：\n"));
        for child in command
            .get_subcommands()
            .filter(|child| !child.is_hide_set())
        {
            let mut child_path = path.to_vec();
            child_path.push(child.get_name().to_owned());
            output.push_str(&format!(
                "  {:<18} {}\n",
                child.get_name(),
                command_description(&child_path, language).unwrap_or(child.get_name())
            ));
        }
    }
    output.push_str(language.select("\nArguments and options:\n", "\n参数与选项：\n"));
    for arg in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
        let description = argument_description(path, arg, language).unwrap_or_default();
        output.push_str(&format!("  {}\n      {description}", argument_syntax(arg)));
        if arg.is_required_set() {
            output.push_str(language.select(" [required]", " [必需]"));
        }
        let defaults: Vec<_> = arg
            .get_default_values()
            .iter()
            .map(|value| value.to_string_lossy())
            .collect();
        if !defaults.is_empty() {
            output.push_str(&format!(
                " [{}: {}]",
                language.select("default", "默认值"),
                defaults.join(", ")
            ));
        }
        if let Some(values) = arg.get_value_parser().possible_values() {
            let values: Vec<_> = values
                .filter(|value| !value.is_hide_set())
                .map(|value| value.get_name().to_owned())
                .collect();
            if !values.is_empty() {
                output.push_str(&format!(
                    " [{}: {}]",
                    language.select("possible values", "可选值"),
                    values.join(", ")
                ));
            }
        }
        if let Some(constraint) = constraint(arg.get_id().as_str(), language) {
            output.push_str(&format!("\n      {constraint}"));
        }
        output.push('\n');
    }
    output
}

pub fn argument_syntax(arg: &clap::Arg) -> String {
    let mut names = Vec::new();
    if let Some(short) = arg.get_short() {
        names.push(format!("-{short}"));
    }
    if let Some(long) = arg.get_long() {
        names.push(format!("--{long}"));
    }
    let takes_value = arg
        .get_num_args()
        .is_some_and(|arity| arity.max_values() > 0);
    let value_name = arg
        .get_value_names()
        .map(|names| {
            names
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
    if names.is_empty() {
        if arg.is_required_set() {
            format!("<{value_name}>")
        } else {
            format!("[{value_name}]")
        }
    } else {
        let mut syntax = names.join(", ");
        if takes_value {
            syntax.push_str(&format!(" <{value_name}>"));
        }
        syntax
    }
}

pub fn command_description(path: &[String], language: Language) -> Option<&'static str> {
    let name = path.last()?.as_str();
    let pair = match name {
        "daily" => (
            "Show daily token and estimated-cost usage",
            "显示每日 token 与估算成本",
        ),
        "weekly" => (
            "Group usage by Monday week start",
            "按周一周起始日期汇总用量",
        ),
        "monthly" => ("Group usage by month", "按月汇总用量"),
        "session" => ("Show per-session usage", "按会话汇总用量"),
        "blocks" => (
            "Show 5-hour blocks and burn-rate projections",
            "显示 5 小时用量窗口与消耗速率预测",
        ),
        "claude" | "codex" | "opencode" | "antigravity" | "antigravity-ide" => {
            ("Show reports for this source", "显示该来源的用量报表")
        }
        "statusline" => ("Print a one-line usage summary", "输出单行用量摘要"),
        "lang" => (
            "Choose English or simplified Chinese for CLI and TUI",
            "选择 CLI 和 TUI 的英语或简体中文",
        ),
        "init" => (
            "Initialize the local runtime and database",
            "初始化本地运行时与数据库",
        ),
        "sync" if path.first().map(String::as_str) == Some("remote") => (
            "Import shards from registered SSH hosts",
            "从已注册的 SSH 主机导入分片",
        ),
        "sync" => (
            "Import local usage and registered SSH remotes",
            "导入本地用量与已注册的 SSH 远端数据",
        ),
        "remote" => ("Manage SSH remote hosts", "管理 SSH 远端主机"),
        "add" => (
            "Register an SSH host after a protocol handshake",
            "协议握手通过后注册 SSH 主机",
        ),
        "list" => ("List registered hosts", "列出已注册主机"),
        "remove" => (
            "Remove a host; keep usage unless explicitly requested",
            "移除主机；默认保留已导入用量",
        ),
        "handshake" => (
            "Print shard protocol and schema versions as JSON",
            "以 JSON 输出分片协议与数据库版本",
        ),
        "status" if path.first().map(String::as_str) == Some("catalog") => (
            "Show base, overlay and effective catalog layers",
            "显示基础、覆盖与生效价格目录层",
        ),
        "status" => (
            "Show database, sources and recent runs",
            "显示数据库、来源与最近运行状态",
        ),
        "source-status" => (
            "Show parser-backed and monitor-only source status",
            "显示解析来源与仅监控平台状态",
        ),
        "diagnostics" => (
            "Export diagnostics JSON or forget a source file",
            "导出诊断 JSON 或忽略来源文件",
        ),
        "doctor" => (
            "Check runtime health and local pricing",
            "检查运行状态与本地价格",
        ),
        "catalog" => (
            "Manage local pricing catalog overlays",
            "管理本地价格目录覆盖",
        ),
        "apply" => (
            "Validate and activate a local v2 overlay",
            "校验并启用本地 v2 覆盖文件",
        ),
        "reset" => (
            "Remove the overlay and restore the base catalog",
            "移除覆盖并恢复基础价格目录",
        ),
        "clean" => (
            "Show runtime disk usage; --yes deletes expired copies",
            "显示运行时磁盘占用；--yes 删除过期副本",
        ),
        "logs" => (
            "Query structured runtime logs and recent runs",
            "查询结构化运行日志与最近运行记录",
        ),
        "serve" => ("Start the browser dashboard", "启动浏览器看板"),
        "dash" => ("Open the terminal dashboard", "打开交互式终端看板"),
        "tui" => ("Deprecated alias for dash", "dash 的已弃用兼容别名"),
        "export" => ("Export usage reports", "导出用量报表"),
        "html" => ("Write an offline dashboard bundle", "写入离线看板文件包"),
        "uninstall" => (
            "Remove legacy integrations; --purge removes runtime data",
            "清理遗留集成；--purge 删除运行时数据",
        ),
        "update" => ("Update from the official repository", "从官方仓库更新"),
        "codex-tracer" => (
            "Start the detailed Codex usage tracker",
            "启动 Codex 详细用量跟踪器",
        ),
        "help" => ("Show help for a command", "显示命令帮助"),
        _ => return None,
    };
    Some(language.select(pair.0, pair.1))
}

pub fn argument_description(
    path: &[String],
    arg: &clap::Arg,
    language: Language,
) -> Option<String> {
    let id = arg.get_id().as_str();
    let zh = match id {
        "home" => "覆盖 LLMUSAGE_HOME 与默认 ~/.llmusage 运行时根目录。",
        "help" => "显示帮助。",
        "version" => "输出版本。",
        "language" => "界面语言：en、zh；zh-CN 规范化为 zh。",
        "since" if path.first().map(String::as_str) == Some("sync") => {
            "--emit-shards 的 RFC 3339 时间下界。"
        }
        "since" => "报表包含式开始日期，格式 YYYY-MM-DD 或 YYYYMMDD。",
        "until" => "报表包含式结束日期，格式 YYYY-MM-DD 或 YYYYMMDD。",
        "json" => "输出机器可读 JSON。",
        "breakdown" => "包含按模型拆分的用量。",
        "order" => "按周期或活动排序。",
        "timezone" => "报表时区：UTC、local（当前固定偏移）或 +08:00 等固定偏移。",
        "locale" => "保留的报表 locale 参数；不控制 CLI/TUI 界面语言。",
        "compact" => "使用紧凑表格布局。",
        "no_cost" => "从报表文本和 JSON 中隐藏成本。",
        "source" => "限制为一个来源标识。",
        "host" => "限制为一个已注册主机标签。",
        "by_agent" => "在统一报表 JSON 中增加按来源行。",
        "sections" => "在一份报表中包含其他周期段。",
        "all" => "显示完整每日历史，代替默认最近 7 天。",
        "instances" => "按项目或实例汇总每日用量。",
        "project" => "按项目标签、哈希或引用过滤。",
        "id" => "按完整或部分会话标识过滤。",
        "active" => "只显示当前活跃窗口。",
        "recent" => "只显示最近三天与活跃窗口。",
        "token_limit" => "token 限额：正整数或 max（历史最大值）。",
        "session_length" => "会话或窗口时长（小时），必须为有限正数。",
        "cache" => "启用状态行缓存写入。",
        "no_cache" => "禁用状态行缓存回退与写入。",
        "refresh_interval" => "缓存刷新间隔（秒），必须为正整数。",
        "cost_source" => "选择优先使用的本地成本来源。",
        "rebuild" => "从本地来源文件或数据库重建用量。",
        "allow_lossy_rebuild" => "允许 --rebuild 在原始来源缺失时删除已导入用量；默认拒绝。",
        "recent_days" => "只导入最近 N 天 UTC 窗口；不推进全历史游标。",
        "parallelism" => "解析工作线程并发数（1 到 32）。",
        "provider_map" => "用于中继提供商归属的 CCR 激活 JSONL 路径。",
        "json_events" => "在 stdout 输出同步生命周期 NDJSON。",
        "verbose" => "显示按来源的管线明细表（文件、事件、字节、耗时），替代 token 与费用汇总。",
        "emit_shards" => "在 stdout 输出标准分片 NDJSON，不打开用户数据库。",
        "out" => "写入的输出路径。",
        "forget_file" => "将绝对路径标记为 deleted_by_user 并删除其游标；来源有歧义时需 --source。",
        "refresh_pricing" => "从本地 litellm 价格文件刷新并重算事件成本；不接受 URL。",
        "path" => "本地 v2 价格覆盖文件路径。",
        "yes" if path.first().map(String::as_str) == Some("remote") => {
            "确认删除已导入用量；--delete-usage 时必需。"
        }
        "yes" => "删除过期迁移副本与研究基线。",
        "limit" => "返回的最近日志与运行记录数量。",
        "level" => "包含的最低日志级别。",
        "command" if path.first().map(String::as_str) == Some("remote") => {
            "远端 argv 前缀；空格拆分，不经本地 shell 执行。"
        }
        "command" => "按命令标签过滤，例如 sync。",
        "port" => "监听端口。",
        "public" => "监听所有网络接口，将看板与 JSON API 暴露到网络。",
        "no_open" => "不自动打开浏览器。",
        "purge" => "同时删除运行时根目录及其数据。",
        "check" => "仅预览 Cargo 安装命令，不更新。",
        "channel" => "更新分支：main 为稳定版；dev 包含未发布改动。",
        "label" => "注册主机标签。",
        "ssh_target" => "SSH 连接目标。",
        "delete_usage" => "同时删除该主机已导入的用量。",
        "subcommand" => "需要显示帮助的命令路径。",
        _ => return None,
    };
    Some(match language {
        Language::Chinese => zh.to_owned(),
        Language::English => {
            if id == "locale" {
                "Reserved report locale parameter; does not select CLI/TUI language.".to_owned()
            } else if id == "language" {
                "UI language: en or zh; zh-CN is normalized to zh.".to_owned()
            } else {
                arg.get_help().map(ToString::to_string).unwrap_or_else(|| {
                    match id {
                        "label" => "Registered host label.",
                        "ssh_target" => "SSH connection target.",
                        "path" => "Local v2 pricing overlay path.",
                        "out" => "Output path.",
                        "json" => "Emit machine-readable JSON.",
                        "port" => "Listening port.",
                        "host" => "Registered host label.",
                        "purge" => "Also remove the runtime root and its data.",
                        _ => "Command argument.",
                    }
                    .to_owned()
                })
            }
        }
    })
}

pub fn constraint(id: &str, language: Language) -> Option<&'static str> {
    let pair = match id {
        "since" | "until" => (
            "Use a valid YYYY-MM-DD or YYYYMMDD date (RFC 3339 for sync --since).",
            "请使用有效 YYYY-MM-DD 或 YYYYMMDD 日期（sync --since 使用 RFC 3339）。",
        ),
        "timezone" => (
            "Use UTC, local, or a fixed offset between -23:59 and +23:59.",
            "请使用 UTC、local，或 -23:59 到 +23:59 之间的固定偏移。",
        ),
        "locale" => (
            "Use en-US, zh-CN or ja-JP.",
            "请使用 en-US、zh-CN 或 ja-JP。",
        ),
        "token_limit" => ("Use a positive integer or max.", "请使用正整数或 max。"),
        "session_length" => ("Use a finite positive number.", "请使用有限正数。"),
        "refresh_interval" => ("Use a positive integer.", "请使用正整数。"),
        "recent_days" => (
            "Use an integer between 1 and 3650.",
            "请使用 1 到 3650 之间的整数。",
        ),
        "parallelism" => (
            "Use an integer between 1 and 32.",
            "请使用 1 到 32 之间的整数。",
        ),
        "port" => (
            "Use an integer between 0 and 65535.",
            "请使用 0 到 65535 之间的整数。",
        ),
        "limit" => ("Use a non-negative integer.", "请使用非负整数。"),
        _ => return None,
    };
    Some(language.select(pair.0, pair.1))
}

const DEFAULT_TERMINAL_WIDTH: usize = 100;
const MIN_TERMINAL_WIDTH: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpLanguage {
    English,
    Chinese,
}

#[derive(Debug, Clone, Copy)]
struct TableSpec<'a> {
    headers: [&'a str; 2],
    rows: &'a [(&'a str, &'a str)],
}

#[derive(Debug, Clone, Copy)]
struct ColumnWidths {
    left: usize,
    right: usize,
}

pub fn top_level_help(language: HelpLanguage) -> String {
    let width = terminal_width();
    top_level_help_with_width(language, width)
}

fn terminal_width() -> usize {
    crossterm::terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
        .filter(|value| *value >= MIN_TERMINAL_WIDTH)
        .or_else(|| {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value >= MIN_TERMINAL_WIDTH)
        })
        .unwrap_or(DEFAULT_TERMINAL_WIDTH)
}

fn top_level_help_with_width(language: HelpLanguage, width: usize) -> String {
    match language {
        HelpLanguage::English => render_help(
            HelpContent {
                title: "llmusage — local-first usage analytics for AI coding CLIs",
                usage_title: "Usage:",
                usage_lines: &[
                    "llmusage [OPTIONS] [COMMAND]",
                    "llmusage help [--zh]",
                    "llmusage help <COMMAND>",
                ],
                common_title: "Common commands:",
                common: TableSpec {
                    headers: ["Command", "What it does"],
                    rows: ENGLISH_COMMANDS,
                },
                global_title: "Global options:",
                global: TableSpec {
                    headers: ["Option", "Meaning"],
                    rows: ENGLISH_GLOBAL_OPTIONS,
                },
                report_title: "Report options:",
                report: TableSpec {
                    headers: ["Option", "Meaning"],
                    rows: ENGLISH_REPORT_OPTIONS,
                },
                examples_title: "Examples:",
                examples: TableSpec {
                    headers: ["Goal", "Command"],
                    rows: ENGLISH_EXAMPLES,
                },
            },
            width,
        ),
        HelpLanguage::Chinese => render_help(
            HelpContent {
                title: "llmusage — 本地优先的 AI CLI 用量分析工具",
                usage_title: "用法：",
                usage_lines: &[
                    "llmusage [OPTIONS] [COMMAND]",
                    "llmusage help [--zh]",
                    "llmusage help <COMMAND>",
                ],
                common_title: "常用命令：",
                common: TableSpec {
                    headers: ["命令", "作用"],
                    rows: CHINESE_COMMANDS,
                },
                global_title: "全局参数：",
                global: TableSpec {
                    headers: ["参数", "含义"],
                    rows: CHINESE_GLOBAL_OPTIONS,
                },
                report_title: "报表参数：",
                report: TableSpec {
                    headers: ["参数", "含义"],
                    rows: CHINESE_REPORT_OPTIONS,
                },
                examples_title: "示例：",
                examples: TableSpec {
                    headers: ["目标", "命令"],
                    rows: CHINESE_EXAMPLES,
                },
            },
            width,
        ),
    }
}

/// Detects top-level help forms that should use llmusage's hand-written table.
///
/// Clap command-specific help remains responsible for `help <COMMAND>` and
/// `<COMMAND> --help`; keep this matcher intentionally narrow.
pub fn is_top_level_help_request<I, S>(args: I) -> Option<HelpLanguage>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => Some(HelpLanguage::English),
        [command] if command == "help" => Some(HelpLanguage::English),
        [command, flag] if command == "help" && flag == "--zh" => Some(HelpLanguage::Chinese),
        _ => None,
    }
}

struct HelpContent<'a> {
    title: &'a str,
    usage_title: &'a str,
    usage_lines: &'a [&'a str],
    common_title: &'a str,
    common: TableSpec<'a>,
    global_title: &'a str,
    global: TableSpec<'a>,
    report_title: &'a str,
    report: TableSpec<'a>,
    examples_title: &'a str,
    examples: TableSpec<'a>,
}

fn render_help(content: HelpContent<'_>, terminal_width: usize) -> String {
    let table_width = terminal_width.clamp(MIN_TERMINAL_WIDTH, 140);
    let mut out = String::new();
    push_line(&mut out, content.title);
    push_line(&mut out, "");
    push_line(&mut out, content.usage_title);
    for line in content.usage_lines {
        push_line(&mut out, &format!("  {line}"));
    }
    push_line(&mut out, "");
    render_section(&mut out, content.common_title, content.common, table_width);
    render_section(&mut out, content.global_title, content.global, table_width);
    render_section(&mut out, content.report_title, content.report, table_width);
    render_section(
        &mut out,
        content.examples_title,
        content.examples,
        table_width,
    );
    out
}

fn render_section(out: &mut String, title: &str, table: TableSpec<'_>, table_width: usize) {
    push_line(out, title);
    push_str(out, &render_table(table, table_width));
    push_line(out, "");
}

fn render_table(table: TableSpec<'_>, table_width: usize) -> String {
    let widths = choose_widths(table, table_width);
    let border = border_line('├', '┼', '┤', widths);
    let mut out = String::new();
    push_line(&mut out, &border_line('┌', '┬', '┐', widths));
    push_wrapped_row(&mut out, table.headers[0], table.headers[1], widths);
    push_line(&mut out, &border);
    for (left, right) in table.rows {
        push_wrapped_row(&mut out, left, right, widths);
    }
    push_line(&mut out, &border_line('└', '┴', '┘', widths));
    out
}

fn choose_widths(table: TableSpec<'_>, table_width: usize) -> ColumnWidths {
    let max_left_content = std::iter::once(table.headers[0])
        .chain(table.rows.iter().map(|(left, _)| *left))
        .map(display_width)
        .max()
        .unwrap_or(0);
    let available = table_width.saturating_sub(7).max(20);
    let max_left = (available / 3).clamp(12, 30);
    let left = max_left_content.clamp(10, max_left);
    let right = available.saturating_sub(left).max(20);
    ColumnWidths { left, right }
}

fn border_line(left: char, middle: char, right: char, widths: ColumnWidths) -> String {
    format!(
        "{left}{}{}{}{right}",
        "─".repeat(widths.left + 2),
        middle,
        "─".repeat(widths.right + 2),
    )
}

fn push_wrapped_row(out: &mut String, left: &str, right: &str, widths: ColumnWidths) {
    let left_lines = wrap_cell(left, widths.left);
    let right_lines = wrap_cell(right, widths.right);
    let height = left_lines.len().max(right_lines.len());
    for index in 0..height {
        let left_part = left_lines.get(index).map(String::as_str).unwrap_or("");
        let right_part = right_lines.get(index).map(String::as_str).unwrap_or("");
        push_line(
            out,
            &format!(
                "│ {} │ {} │",
                pad_cell(left_part, widths.left),
                pad_cell(right_part, widths.right)
            ),
        );
    }
}

fn wrap_cell(value: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for logical_line in value.lines() {
        wrap_logical_line(logical_line, width, &mut lines);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_logical_line(value: &str, width: usize, lines: &mut Vec<String>) {
    let mut current = String::new();
    for word in value.split_whitespace() {
        let separator = usize::from(!current.is_empty());
        if display_width(&current) + separator + display_width(word) <= width {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            continue;
        }

        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if display_width(word) <= width {
            current.push_str(word);
        } else {
            split_long_word(word, width, lines, &mut current);
        }
    }
    if !current.is_empty() || value.is_empty() {
        lines.push(current);
    }
}

fn split_long_word(word: &str, width: usize, lines: &mut Vec<String>, current: &mut String) {
    for ch in word.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if !current.is_empty() && display_width(current.as_str()) + ch_width > width {
            lines.push(std::mem::take(current));
        }
        current.push(ch);
    }
}

fn pad_cell(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(display_width(value));
    format!("{value}{}", " ".repeat(padding))
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

fn push_str(out: &mut String, value: &str) {
    out.push_str(value);
}

const ENGLISH_COMMANDS: &[(&str, &str)] = &[
    (
        "lang [en|zh]",
        "Choose English or simplified Chinese for CLI and TUI.",
    ),
    ("codex-tracer", "Start the detailed Codex usage tracker."),
    (
        "llmusage / daily",
        "Show daily token and estimated-cost usage; defaults to the last 7 days.",
    ),
    ("monthly", "Group token and estimated-cost usage by month."),
    (
        "weekly",
        "Group token and estimated-cost usage by Monday week start.",
    ),
    (
        "session",
        "Group usage by source session, optionally filtered by project or session id.",
    ),
    (
        "blocks",
        "Show 5-hour usage blocks and burn-rate projections.",
    ),
    (
        "claude / codex / opencode / antigravity / antigravity-ide",
        "Show focused daily, weekly, monthly, or session reports for one source.",
    ),
    (
        "statusline",
        "Print a single statusline-friendly usage summary.",
    ),
    (
        "init",
        "Create the local runtime and bootstrap the usage database.",
    ),
    (
        "sync",
        "Passively import local usage artifacts, then pull registered SSH remotes.",
    ),
    (
        "remote",
        "Register, list, remove, or import SSH remote hosts.",
    ),
    ("status", "Print database, source, and recent-run status."),
    (
        "source-status",
        "Print parser-backed source and monitor-only platform status.",
    ),
    (
        "diagnostics",
        "Emit diagnostics JSON or forget an intentionally ignored source file.",
    ),
    (
        "doctor",
        "Run health checks and optionally refresh pricing from a local file.",
    ),
    (
        "catalog",
        "Apply, inspect, or reset the local pricing catalog overlay.",
    ),
    (
        "logs",
        "Query local structured runtime logs and recent run records.",
    ),
    (
        "clean",
        "Show runtime disk usage; --yes deletes expired database copies.",
    ),
    ("dash", "Open the interactive terminal dashboard."),
    (
        "serve",
        "Start the browser dashboard; binds 127.0.0.1 by default.",
    ),
    ("export html", "Write an offline dashboard bundle."),
    (
        "update [main|dev]",
        "Preview or install the latest official stable/development branch.",
    ),
    (
        "uninstall",
        "Clean up legacy hooks/plugins; --purge also removes the runtime root.",
    ),
];

const ENGLISH_GLOBAL_OPTIONS: &[(&str, &str)] = &[
    (
        "--home <PATH>",
        "Override LLMUSAGE_HOME and the default ~/.llmusage runtime root.",
    ),
    (
        "-h, --help",
        "Show this top-level table. Use `help <COMMAND>` for command-specific help.",
    ),
    ("-V, --version", "Print version."),
];

const ENGLISH_REPORT_OPTIONS: &[(&str, &str)] = &[
    (
        "--since <YYYY-MM-DD|YYYYMMDD>",
        "Inclusive report start date. Alias: -s.",
    ),
    (
        "--until <YYYY-MM-DD|YYYYMMDD>",
        "Inclusive report end date. Alias: -u.",
    ),
    (
        "--json",
        "Emit stable JSON for supported report commands. Alias: -j.",
    ),
    (
        "--breakdown",
        "Include per-model breakdown rows or payloads where supported. Alias: -b.",
    ),
    ("--order asc|desc", "Sort report rows by period/activity."),
    (
        "--timezone <TZ>",
        "Report timezone: UTC, local (current fixed offset), or a fixed offset like +08:00. Alias: -z.",
    ),
    (
        "--locale <LOCALE>",
        "Reserved report locale parameter; use lang for CLI/TUI language. Alias: -l.",
    ),
    ("--compact", "Use a narrower table layout."),
    (
        "--no-cost",
        "Hide cost columns and cost fields from report output.",
    ),
    (
        "--source <SOURCE>",
        "Restrict reports or sync to codex, claude, opencode, antigravity, antigravity_ide, kimi_code, pi, omp, cindy, grok, zcode, or deepseek_harness.",
    ),
    (
        "--host <LABEL>",
        "Restrict reports to one registered host label.",
    ),
    (
        "-A, --by-agent",
        "Add per-source rows to unified report JSON; text already includes Agent rows.",
    ),
    (
        "--sections <SECTION,...>",
        "Include daily, weekly, monthly, or session sections in one report.",
    ),
    (
        "--all",
        "Show full daily history instead of the default last 7 days.",
    ),
    ("-i, --instances", "Group daily rows by project/instance."),
    (
        "-p, --project <PROJECT>",
        "Filter by project label, hash, or reference.",
    ),
];

const ENGLISH_EXAMPLES: &[(&str, &str)] = &[
    ("Initialize and sync", "llmusage init && llmusage sync"),
    ("Default daily report", "llmusage"),
    (
        "Date-limited Codex report",
        "llmusage daily --source codex --since 20260501 --until 20260518",
    ),
    (
        "JSON monthly breakdown",
        "llmusage monthly --json --breakdown",
    ),
    ("Weekly report", "llmusage weekly --by-agent --json"),
    ("Focused Codex report", "llmusage codex daily --no-cost"),
    ("Active burn-rate block", "llmusage blocks --active"),
    ("Browser dashboard", "llmusage serve"),
    ("Inspect pricing catalog", "llmusage catalog status --json"),
    (
        "Offline export",
        "llmusage export html --out ./llmusage-report",
    ),
    ("Chinese top-level help", "llmusage help --zh"),
    ("Command-specific help", "llmusage help daily"),
];

const CHINESE_COMMANDS: &[(&str, &str)] = &[
    ("lang [en|zh]", "选择 CLI 和 TUI 的英语或简体中文。"),
    ("codex-tracer", "启动 Codex 详细用量跟踪器。"),
    (
        "llmusage / daily",
        "显示 daily token 与估算成本；默认最近 7 天。",
    ),
    ("monthly", "按月汇总 token 与估算成本。"),
    ("weekly", "按周一周起始日期汇总 token 与估算成本。"),
    (
        "session",
        "按来源 session 汇总，可按项目或 session id 过滤。",
    ),
    ("blocks", "显示 5 小时用量窗口和 burn-rate 预测。"),
    (
        "claude / codex / opencode / antigravity / antigravity-ide",
        "显示单一来源的聚焦 daily、weekly、monthly 或 session 报表。",
    ),
    ("statusline", "输出一行适合 statusline 的用量摘要。"),
    ("init", "创建本地运行时并引导用量数据库。"),
    (
        "sync",
        "被动导入本地用量记录，然后拉取已注册的 SSH 远端主机。",
    ),
    ("remote", "注册、列出、移除或导入 SSH 远端主机。"),
    ("status", "输出数据库、来源与最近运行状态。"),
    ("source-status", "输出解析器支持的来源与仅监控平台状态。"),
    ("diagnostics", "输出诊断 JSON，或显式忽略某个来源文件。"),
    ("doctor", "运行健康检查，也可从本地文件刷新价格。"),
    ("catalog", "应用、查看或重置本地价格目录覆盖。"),
    ("logs", "查询本地结构化运行日志与最近命令记录。"),
    (
        "clean",
        "以表格总览运行时占用；--yes 才删除过期数据库副本。",
    ),
    ("dash", "打开交互式终端 Dashboard。"),
    ("serve", "启动浏览器 Dashboard；默认绑定 127.0.0.1。"),
    ("export html", "写入离线 Dashboard bundle。"),
    (
        "update [main|dev]",
        "预览或安装官方稳定/开发分支的最新版本。",
    ),
    (
        "uninstall",
        "清理遗留 hook/plugin；--purge 还会删除运行时根目录。",
    ),
];

const CHINESE_GLOBAL_OPTIONS: &[(&str, &str)] = &[
    (
        "--home <PATH>",
        "覆盖 LLMUSAGE_HOME 与默认 ~/.llmusage 运行时根目录。",
    ),
    (
        "-h, --help",
        "显示这个顶层表格；子命令 help 用 `help <COMMAND>`。",
    ),
    ("-V, --version", "输出版本。"),
];

const CHINESE_REPORT_OPTIONS: &[(&str, &str)] = &[
    (
        "--since <YYYY-MM-DD|YYYYMMDD>",
        "报表包含式开始日期；别名：-s。",
    ),
    (
        "--until <YYYY-MM-DD|YYYYMMDD>",
        "报表包含式结束日期；别名：-u。",
    ),
    ("--json", "支持的报表命令输出稳定 JSON；别名：-j。"),
    (
        "--breakdown",
        "在支持处包含按模型拆分的行或 payload；别名：-b。",
    ),
    ("--order asc|desc", "按周期/活动排序报表行。"),
    (
        "--timezone <TZ>",
        "报表时区：UTC、local（当前固定偏移）或 +08:00 这类固定偏移；别名：-z。",
    ),
    (
        "--locale <LOCALE>",
        "保留的报表 locale 参数；界面语言使用 lang 设置；别名：-l。",
    ),
    ("--compact", "使用更窄的表格布局。"),
    ("--no-cost", "从报表输出隐藏成本列和成本字段。"),
    (
        "--source <SOURCE>",
        "报表或同步限制到 codex、claude、opencode、antigravity、antigravity_ide、kimi_code、pi、omp、cindy、grok、zcode 或 deepseek_harness。",
    ),
    ("--host <LABEL>", "报表限制到一个已注册主机 label。"),
    (
        "-A, --by-agent",
        "统一报表 JSON 增加按来源行；文本已默认包含 Agent 行。",
    ),
    (
        "--sections <SECTION,...>",
        "在一份报表中组合 daily、weekly、monthly 或 session 段。",
    ),
    ("--all", "daily 显示完整历史，而不是默认最近 7 天。"),
    ("-i, --instances", "daily 按项目/实例分组。"),
    (
        "-p, --project <PROJECT>",
        "按项目 label、hash 或 reference 过滤。",
    ),
];

const CHINESE_EXAMPLES: &[(&str, &str)] = &[
    ("初始化并同步", "llmusage init && llmusage sync"),
    ("默认 daily 报表", "llmusage"),
    (
        "指定日期的 Codex 报表",
        "llmusage daily --source codex --since 20260501 --until 20260518",
    ),
    ("JSON 月报拆分", "llmusage monthly --json --breakdown"),
    ("周报 JSON", "llmusage weekly --by-agent --json"),
    ("聚焦 Codex 报表", "llmusage codex daily --no-cost"),
    ("当前 burn-rate 窗口", "llmusage blocks --active"),
    ("浏览器 Dashboard", "llmusage serve"),
    ("查看价格目录", "llmusage catalog status --json"),
    ("离线导出", "llmusage export html --out ./llmusage-report"),
    ("设置英语界面", "llmusage lang en"),
    ("子命令 help", "llmusage help daily"),
];

#[cfg(test)]
mod tests {
    use super::{
        HelpLanguage, display_width, is_top_level_help_request, top_level_help_with_width,
    };

    #[test]
    fn detects_only_top_level_help_requests() {
        assert_eq!(
            is_top_level_help_request(["--help"]),
            Some(HelpLanguage::English)
        );
        assert_eq!(
            is_top_level_help_request(["-h"]),
            Some(HelpLanguage::English)
        );
        assert_eq!(
            is_top_level_help_request(["help"]),
            Some(HelpLanguage::English)
        );
        assert_eq!(
            is_top_level_help_request(["help", "--zh"]),
            Some(HelpLanguage::Chinese)
        );
        assert_eq!(is_top_level_help_request(["help", "daily"]), None);
        assert_eq!(is_top_level_help_request(["daily", "--help"]), None);
    }

    #[test]
    fn renders_terminal_tables_without_markdown_separator_rows() {
        let help = top_level_help_with_width(HelpLanguage::English, 80);
        assert!(help.contains("┌"));
        assert!(help.contains("│ Command"));
        assert!(help.contains("source-status"));
        assert!(help.contains("clean"));
        assert!(help.contains("remote"));
        assert!(help.contains("catalog"));
        assert!(help.contains("claude"));
        assert!(help.contains("antigravity"));
        assert!(help.contains("Report options:"));
        assert!(help.contains("llmusage help --zh"));
        assert!(!help.contains("| --- |"));
        assert!(help.lines().all(|line| display_width(line) <= 80));
    }

    #[test]
    fn renders_chinese_with_unicode_width_wrapping() {
        let zh = top_level_help_with_width(HelpLanguage::Chinese, 70);
        assert!(zh.contains("│ 命令"));
        assert!(zh.contains("source-status"));
        assert!(zh.contains("clean"));
        assert!(zh.contains("remote"));
        assert!(zh.contains("catalog"));
        assert!(zh.contains("claude"));
        assert!(zh.contains("antigravity"));
        assert!(zh.contains("报表参数"));
        assert!(zh.contains("示例"));
        assert!(!zh.contains("| --- |"));
        assert!(zh.lines().all(|line| display_width(line) <= 70));
    }
}
