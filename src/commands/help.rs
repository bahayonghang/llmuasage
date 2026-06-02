use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
        "llmusage / daily",
        "Show daily token and estimated-cost usage; defaults to the last 7 days.",
    ),
    ("monthly", "Group token and estimated-cost usage by month."),
    (
        "session",
        "Group usage by source session, optionally filtered by project or session id.",
    ),
    (
        "blocks",
        "Show 5-hour usage blocks and burn-rate projections.",
    ),
    (
        "statusline",
        "Print a single statusline-friendly usage summary.",
    ),
    (
        "init",
        "Create the local runtime and install/probe supported integrations.",
    ),
    (
        "sync",
        "Import local Codex, Claude, OpenCode, and Antigravity usage artifacts.",
    ),
    (
        "status",
        "Print database, source, integration, and recent-run status.",
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
        "logs",
        "Query local structured runtime logs and recent run records.",
    ),
    ("dash", "Open the interactive terminal dashboard."),
    ("serve", "Start the local browser dashboard on 127.0.0.1."),
    ("export html", "Write an offline dashboard bundle."),
    (
        "uninstall",
        "Restore integrations; --purge also removes the runtime root.",
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
        "--since <YYYYMMDD>",
        "Inclusive report start date. Alias: -s.",
    ),
    (
        "--until <YYYYMMDD>",
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
        "Report timezone: UTC, local, or a fixed offset like +08:00. Alias: -z.",
    ),
    (
        "--locale <LOCALE>",
        "Lightweight locale selector for titles and number formatting. Alias: -l.",
    ),
    ("--compact", "Use a narrower table layout."),
    (
        "--source <SOURCE>",
        "Restrict reports or sync to codex, claude, opencode, or antigravity.",
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
    ("Active burn-rate block", "llmusage blocks --active"),
    ("Browser dashboard", "llmusage serve"),
    (
        "Offline export",
        "llmusage export html --out ./llmusage-report",
    ),
    ("Chinese top-level help", "llmusage help --zh"),
    ("Command-specific help", "llmusage help daily"),
];

const CHINESE_COMMANDS: &[(&str, &str)] = &[
    (
        "llmusage / daily",
        "显示 daily token 与估算成本；默认最近 7 天。",
    ),
    ("monthly", "按月汇总 token 与估算成本。"),
    (
        "session",
        "按来源 session 汇总，可按项目或 session id 过滤。",
    ),
    ("blocks", "显示 5 小时用量窗口和 burn-rate 预测。"),
    ("statusline", "输出一行适合 statusline 的用量摘要。"),
    ("init", "创建本地运行时并安装/探测支持的集成。"),
    (
        "sync",
        "导入本地 Codex、Claude、OpenCode 与 Antigravity 用量记录。",
    ),
    ("status", "输出数据库、来源、集成与最近运行状态。"),
    ("diagnostics", "输出诊断 JSON，或显式忽略某个来源文件。"),
    ("doctor", "运行健康检查，也可从本地文件刷新价格。"),
    ("logs", "查询本地结构化运行日志与最近命令记录。"),
    ("dash", "打开交互式终端 Dashboard。"),
    ("serve", "在 127.0.0.1 启动本地浏览器 Dashboard。"),
    ("export html", "写入离线 Dashboard bundle。"),
    ("uninstall", "恢复集成；--purge 还会删除运行时根目录。"),
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
    ("--since <YYYYMMDD>", "报表包含式开始日期；别名：-s。"),
    ("--until <YYYYMMDD>", "报表包含式结束日期；别名：-u。"),
    ("--json", "支持的报表命令输出稳定 JSON；别名：-j。"),
    (
        "--breakdown",
        "在支持处包含按模型拆分的行或 payload；别名：-b。",
    ),
    ("--order asc|desc", "按周期/活动排序报表行。"),
    (
        "--timezone <TZ>",
        "报表时区：UTC、local 或 +08:00 这类固定偏移；别名：-z。",
    ),
    (
        "--locale <LOCALE>",
        "标题和数字格式的轻量 locale 选择；别名：-l。",
    ),
    ("--compact", "使用更窄的表格布局。"),
    (
        "--source <SOURCE>",
        "报表或同步限制到 codex、claude、opencode 或 antigravity。",
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
    ("当前 burn-rate 窗口", "llmusage blocks --active"),
    ("浏览器 Dashboard", "llmusage serve"),
    ("离线导出", "llmusage export html --out ./llmusage-report"),
    ("英文顶层 help", "llmusage help"),
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
        assert!(help.contains("Report options:"));
        assert!(help.contains("llmusage help --zh"));
        assert!(!help.contains("| --- |"));
        assert!(help.lines().all(|line| display_width(line) <= 80));
    }

    #[test]
    fn renders_chinese_with_unicode_width_wrapping() {
        let zh = top_level_help_with_width(HelpLanguage::Chinese, 70);
        assert!(zh.contains("│ 命令"));
        assert!(zh.contains("报表参数"));
        assert!(zh.contains("示例"));
        assert!(!zh.contains("| --- |"));
        assert!(zh.lines().all(|line| display_width(line) <= 70));
    }
}
