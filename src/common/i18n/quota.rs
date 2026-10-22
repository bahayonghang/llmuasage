//! Paired copy for the subscription quota presentation boundary.
use super::{Language, current, text};
use crate::subscription::{DiagnosticHint, MetricHint, RemainingHint, WindowHint};

#[derive(Clone, Copy)]
pub(crate) enum Msg {
    UsageSyncing,
    Usage,
    Fetching,
    NotLoaded,
    RefreshSyncing,
    Refresh,
    ShowEmails,
    HideEmails,
    ActionsBar,
    SyncStatus,
    Empty,
    LoginHint,
    Summary,
    State,
    Active,
    Capacity,
    Fallback,
    NextReset,
    Action,
    Diagnostics,
    Attention,
    NoAttention,
    Providers,
    Selected,
    Status,
    Unknown,
    Email,
    Credential,
    Limits,
    NoMetrics,
    Actions,
    ExternallyManagedIndented,
    Accounts,
    NarrowHeader,
    Provider,
    Account,
    Plan,
    Auth,
    Health,
    Limit,
    Reset,
    HiddenEmail,
    OpenCodeManaged,
    ExternallyManaged,
    Managed,
    Ready,
    Watch,
    QuotaLow,
    ReadyWarnings,
    QuotaLowState,
    StayActive,
    ChooseActive,
    NoActive,
    NoFallback,
    NoReset,
    ResetsNow,
}

pub(crate) fn label(id: Msg) -> &'static str {
    match id {
        Msg::UsageSyncing => text("Usage  Syncing", "额度  正在刷新"),
        Msg::Usage => text("Usage", "额度"),
        Msg::Fetching => text("Fetching subscription data...", "正在获取订阅数据…"),
        Msg::NotLoaded => text("No subscription data loaded", "尚未加载订阅数据"),
        Msg::RefreshSyncing => text("r Syncing", "r 正在刷新"),
        Msg::Refresh => text("r Refresh", "r 刷新"),
        Msg::ShowEmails => text("m Show Emails", "m 显示邮箱"),
        Msg::HideEmails => text("m Hide Emails", "m 隐藏邮箱"),
        Msg::ActionsBar => text(" Actions ", " 操作 "),
        Msg::SyncStatus => text(" y Sync status ", " y 同步状态 "),
        Msg::Empty => text("No subscription data available", "暂无订阅数据"),
        Msg::LoginHint => text(
            "Use Refresh after logging into Claude, Codex, Grok, or Kimi.",
            "登录 Claude、Codex、Grok 或 Kimi 后刷新。",
        ),
        Msg::Summary => text("Usage Summary", "额度概览"),
        Msg::State => text("State", "状态"),
        Msg::Active => text("Active", "当前账号"),
        Msg::Capacity => text("Capacity", "可用情况"),
        Msg::Fallback => text("Fallback", "备用账号"),
        Msg::NextReset => text("Next Reset", "下次重置"),
        Msg::Action => text("Action", "建议操作"),
        Msg::Diagnostics => text("Diagnostics", "诊断"),
        Msg::Attention => text("Attention", "需要关注"),
        Msg::NoAttention => text("  No accounts need attention", "  没有需要关注的账号"),
        Msg::Providers => text("Providers", "服务商"),
        Msg::Selected => text("Selected Account", "所选账号"),
        Msg::Status => text("Status", "状态"),
        Msg::Unknown => text("Unknown", "未知"),
        Msg::Email => text("Email", "邮箱"),
        Msg::Credential => text("Credential", "凭据"),
        Msg::Limits => text("Limits", "额度限制"),
        Msg::NoMetrics => text("  No quota metrics returned", "  未返回额度指标"),
        Msg::Actions => text("Actions", "操作"),
        Msg::ExternallyManagedIndented => text("  Managed externally", "  由外部管理"),
        Msg::Accounts => text("Accounts", "账号"),
        Msg::NarrowHeader => text(" #  Account / Status", " #  账号 / 状态"),
        Msg::Provider => text("Provider", "服务商"),
        Msg::Account => text("Account", "账号"),
        Msg::Plan => text("Plan", "方案"),
        Msg::Auth => text("Auth", "认证"),
        Msg::Health => text("Health", "可用状态"),
        Msg::Limit => text("Limit", "额度"),
        Msg::Reset => text("Reset", "重置"),
        Msg::HiddenEmail => text("[hidden email]", "[邮箱已隐藏]"),
        Msg::OpenCodeManaged => text("managed by OpenCode", "由 OpenCode 管理"),
        Msg::ExternallyManaged => text("managed externally", "由外部管理"),
        Msg::Managed => text("Managed", "外部管理"),
        Msg::Ready => text("Ready", "可用"),
        Msg::Watch => text("Watch", "需关注"),
        Msg::QuotaLow => text("Quota Low", "额度偏低"),
        Msg::ReadyWarnings => text("Ready with warnings", "可用但需关注"),
        Msg::QuotaLowState => text("Quota low", "额度偏低"),
        Msg::StayActive => text("Stay on the active account", "继续使用当前账号"),
        Msg::ChooseActive => text("Choose an active account", "选择一个当前账号"),
        Msg::NoActive => text("No active account", "没有当前账号"),
        Msg::NoFallback => text("No ready fallback", "没有可用备用账号"),
        Msg::NoReset => text("No reset data", "无重置数据"),
        Msg::ResetsNow => text("resets now", "即将重置"),
    }
}

pub(crate) fn percent_left(value: f64) -> String {
    text(&format!("{value:.0}% left"), &format!("剩余 {value:.0}%")).to_string()
}
pub(crate) fn metric_label(hint: &MetricHint) -> String {
    let window = match hint.window {
        WindowHint::Session => text("Session", "会话").to_string(),
        WindowHint::Weekly => text("Weekly", "每周").to_string(),
        WindowHint::Opus => "Opus".to_string(),
        WindowHint::Frequent => text("Frequent", "高频").to_string(),
        WindowHint::Occasional => text("Occasional", "低频").to_string(),
        WindowHint::Seconds(604800) => text("Weekly", "每周").to_string(),
        WindowHint::Seconds(seconds) if seconds % 86400 == 0 => {
            format!("{}{}", seconds / 86400, text("d", "天"))
        }
        WindowHint::Seconds(seconds) if seconds % 3600 == 0 => {
            format!("{}{}", seconds / 3600, text("h", "小时"))
        }
        WindowHint::Seconds(seconds) => format!("{seconds}{}", text("s", "秒")),
    };
    match &hint.prefix {
        Some(prefix) => format!(
            "{prefix} {}",
            if current() == Language::English {
                window.to_ascii_lowercase()
            } else {
                window
            }
        ),
        None => window,
    }
}
pub(crate) fn absolute_left(hint: &RemainingHint) -> String {
    let value = match hint {
        RemainingHint::Integer { remaining, limit } => format!("{remaining}/{limit}"),
        RemainingHint::Rounded { remaining, limit } => format!("{remaining:.0}/{limit:.0}"),
    };
    match current() {
        Language::English => format!("{value} left"),
        Language::Chinese => format!("剩余 {value}"),
    }
}
pub(crate) fn original(detail: &str) -> String {
    format!("{}: {detail}", text("Original detail", "原始详情"))
}
pub(crate) fn diagnostic(provider: &str, hint: &DiagnosticHint, canonical: &str) -> String {
    if current() == Language::English {
        return canonical.to_string();
    }
    match hint {
        DiagnosticHint::MissingCredentials => {
            format!("未找到 {provider} 凭据。请运行该服务商 CLI 登录。")
        }
        DiagnosticHint::MissingOauth => {
            format!("没有 {provider} OAuth 凭据。请运行该服务商 CLI 登录。")
        }
        DiagnosticHint::MissingToken => {
            format!("没有 {provider} 访问令牌。请运行该服务商 CLI 登录。")
        }
        DiagnosticHint::CredentialsModified => format!("获取额度期间 {provider} 凭据发生变化"),
        DiagnosticHint::InvalidAuthObject => format!("{provider} auth.json 必须包含一个对象。"),
        DiagnosticHint::Unavailable => format!("{provider} 额度不可用"),
        DiagnosticHint::NoUsage => format!("{provider} 额度不可用：未返回用量或有效订阅数据"),
        DiagnosticHint::Http { status } if matches!(*status, 401 | 403) => format!(
            "{provider} 额度不可用：已保存的访问令牌被拒绝（HTTP {status}）。请运行该服务商 CLI 刷新登录后重试。"
        ),
        DiagnosticHint::Http { status } => format!("{provider} 额度请求失败（HTTP {status}）"),
    }
}
