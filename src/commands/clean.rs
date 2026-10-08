use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use unicode_width::UnicodeWidthStr;

use crate::app::AppContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryLocation {
    Root,
    BackupsChild,
    BaselinesChild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Category {
    LiveDb,
    LiveSidecar,
    Migration,
    ConfigBackup,
    Baseline,
    Logs,
    Cache,
    CodexTracer,
    Bin,
    Exports,
    Lock,
    Pricing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Known(Category),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Keep,
    Delete,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DisplayRow {
    category: String,
    path: String,
    files: usize,
    bytes: u64,
    modified: String,
    action: &'static str,
}

#[derive(Debug, Default)]
struct Acc {
    present: bool,
    files: usize,
    bytes: u64,
    latest: Option<SystemTime>,
}

#[derive(Debug, Default)]
struct Measured {
    files: usize,
    bytes: u64,
    latest: Option<SystemTime>,
}

#[derive(Debug, Default)]
struct Plan {
    categories: BTreeMap<Category, Acc>,
    unknowns: Vec<DisplayRow>,
    delete_files: Vec<PathBuf>,
    baseline_dirs: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct DeleteReport {
    files: usize,
    bytes: u64,
    directories: usize,
    failures: Vec<String>,
}

struct CleanOutcome {
    text: String,
    error: Option<String>,
}

pub async fn run(app: &AppContext, yes: bool) -> Result<()> {
    let outcome = clean_home(&app.paths.root_dir, yes)?;
    print!("{}", outcome.text);
    if let Some(error) = outcome.error {
        bail!("{error}");
    }
    Ok(())
}

fn clean_home(root: &Path, yes: bool) -> Result<CleanOutcome> {
    if !root.exists() {
        return Ok(CleanOutcome {
            text: "没有可清理内容\n".to_string(),
            error: None,
        });
    }
    let plan = scan(root).with_context(|| format!("读取 {} 失败", root.display()))?;
    let rows = rows_from(&plan);
    if rows.is_empty() {
        return Ok(CleanOutcome {
            text: "没有可清理内容\n".to_string(),
            error: None,
        });
    }

    let mut text = render_plan(&rows);
    if !yes {
        text.push_str("未修改磁盘。要删除「可删除」项，请运行 llmusage clean --yes。\n");
        return Ok(CleanOutcome { text, error: None });
    }
    let report = apply_deletes(root, &plan);
    append_delete_summary(&mut text, &report);
    Ok(outcome_from_report(text, report))
}

fn outcome_from_report(text: String, report: DeleteReport) -> CleanOutcome {
    let error = if report.failures.is_empty() {
        None
    } else {
        Some(format!("{} 个路径删除失败", report.failures.len()))
    };
    CleanOutcome { text, error }
}

fn append_delete_summary(text: &mut String, report: &DeleteReport) {
    text.push_str(&format!(
        "已删除 {} 个文件，释放 {}。\n",
        report.files,
        format_byte_size(report.bytes)
    ));
    if report.directories > 0 {
        text.push_str(&format!("已移除 {} 个空目录。\n", report.directories));
    }
    if !report.failures.is_empty() {
        text.push_str("删除失败：\n");
        for failure in &report.failures {
            text.push_str(&format!("- {failure}\n"));
        }
    }
}

fn classify_entry(name: &str, kind: EntryKind, location: EntryLocation) -> (Class, Action) {
    if !matches!(kind, EntryKind::File | EntryKind::Directory) {
        return (Class::Unknown, Action::Unknown);
    }
    match location {
        EntryLocation::BackupsChild => {
            if kind != EntryKind::File {
                return (Class::Unknown, Action::Unknown);
            }
            if name.ends_with(".bak") || name == "codex_notify_original.json" {
                return (Class::Known(Category::ConfigBackup), Action::Keep);
            }
            if name.starts_with("llmusage.db.pre-") {
                return (Class::Known(Category::Migration), Action::Delete);
            }
            (Class::Unknown, Action::Unknown)
        }
        EntryLocation::BaselinesChild => {
            if kind == EntryKind::File {
                (Class::Known(Category::Baseline), Action::Delete)
            } else {
                (Class::Unknown, Action::Unknown)
            }
        }
        EntryLocation::Root => classify_root_name(name, kind),
    }
}

fn classify_root_name(name: &str, kind: EntryKind) -> (Class, Action) {
    let category = match (kind, name) {
        (EntryKind::File, "llmusage.db") => Some(Category::LiveDb),
        (EntryKind::File, "llmusage.db-wal" | "llmusage.db-shm") => Some(Category::LiveSidecar),
        (EntryKind::File, "codex-tracer.db" | "codex-tracer.db-wal" | "codex-tracer.db-shm") => {
            Some(Category::CodexTracer)
        }
        (EntryKind::File, "worker.lock") => Some(Category::Lock),
        (EntryKind::File, "pricing-cache-litellm.json" | "pricing-cache-models-dev.json") => {
            Some(Category::Pricing)
        }
        (EntryKind::Directory, "logs") => Some(Category::Logs),
        (EntryKind::Directory, "cache") => Some(Category::Cache),
        (EntryKind::Directory, "bin") => Some(Category::Bin),
        (EntryKind::Directory, "exports") => Some(Category::Exports),
        (EntryKind::Directory, "pricing") => Some(Category::Pricing),
        (EntryKind::Directory, "baselines") => Some(Category::Baseline),
        _ => None,
    };
    match category {
        Some(category) => (Class::Known(category), action_for(category)),
        None => (Class::Unknown, Action::Unknown),
    }
}

fn action_for(category: Category) -> Action {
    match category {
        Category::Migration | Category::Baseline => Action::Delete,
        _ => Action::Keep,
    }
}

fn scan(root: &Path) -> Result<Plan> {
    let mut plan = Plan::default();
    for entry in read_sorted(root)? {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        let kind = entry_kind(&meta);
        let Ok(name) = entry.file_name().into_string() else {
            push_unknown(&mut plan, root, &path, &meta, true)?;
            continue;
        };
        if matches!(kind, EntryKind::Symlink | EntryKind::Other) {
            push_unknown(&mut plan, root, &path, &meta, false)?;
            continue;
        }
        match (name.as_str(), kind) {
            ("backups", EntryKind::Directory) => scan_backups(&mut plan, root, &path)?,
            ("baselines", EntryKind::Directory) => scan_baselines(&mut plan, root, &path)?,
            ("logs", EntryKind::Directory) => {
                scan_keep_dir(&mut plan, root, &path, Category::Logs)?
            }
            ("cache", EntryKind::Directory) => {
                scan_keep_dir(&mut plan, root, &path, Category::Cache)?
            }
            ("bin", EntryKind::Directory) => scan_keep_dir(&mut plan, root, &path, Category::Bin)?,
            ("exports", EntryKind::Directory) => {
                scan_keep_dir(&mut plan, root, &path, Category::Exports)?
            }
            ("pricing", EntryKind::Directory) => {
                scan_keep_dir(&mut plan, root, &path, Category::Pricing)?
            }
            _ => classify_root_entry(&mut plan, root, &path, &name, kind, &meta)?,
        }
    }
    Ok(plan)
}

fn classify_root_entry(
    plan: &mut Plan,
    root: &Path,
    path: &Path,
    name: &str,
    kind: EntryKind,
    meta: &fs::Metadata,
) -> Result<()> {
    let (class, action) = classify_entry(name, kind, EntryLocation::Root);
    if let (Class::Known(category), Action::Keep, EntryKind::File) = (class, action, kind) {
        add_file(acc_mut(plan, category), meta);
        return Ok(());
    }
    push_unknown(plan, root, path, meta, kind == EntryKind::Directory)
}

fn scan_backups(plan: &mut Plan, root: &Path, dir: &Path) -> Result<()> {
    for entry in read_sorted(dir)? {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        let kind = entry_kind(&meta);
        let Ok(name) = entry.file_name().into_string() else {
            push_unknown(plan, root, &path, &meta, false)?;
            continue;
        };
        let (class, action) = classify_entry(&name, kind, EntryLocation::BackupsChild);
        if let Class::Known(category) = class
            && kind == EntryKind::File
            && matches!(action, Action::Keep | Action::Delete)
        {
            if action == Action::Delete && !is_within_root(root, &path) {
                push_unknown(plan, root, &path, &meta, false)?;
                continue;
            }
            add_file(acc_mut(plan, category), &meta);
            if action == Action::Delete {
                plan.delete_files.push(path);
            }
            continue;
        }
        push_unknown(plan, root, &path, &meta, false)?;
    }
    Ok(())
}

fn scan_baselines(plan: &mut Plan, root: &Path, dir: &Path) -> Result<()> {
    plan.baseline_dirs.push(dir.to_path_buf());
    acc_mut(plan, Category::Baseline).present = true;
    for entry in read_sorted(dir)? {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        let kind = entry_kind(&meta);
        if kind == EntryKind::Directory {
            scan_baselines(plan, root, &path)?;
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let (class, action) = classify_entry(&name, kind, EntryLocation::BaselinesChild);
        if matches!(
            (class, action, kind),
            (
                Class::Known(Category::Baseline),
                Action::Delete,
                EntryKind::File
            )
        ) {
            if !is_within_root(root, &path) {
                push_unknown(plan, root, &path, &meta, false)?;
                continue;
            }
            add_file(acc_mut(plan, Category::Baseline), &meta);
            plan.delete_files.push(path);
            continue;
        }
        push_unknown(plan, root, &path, &meta, false)?;
    }
    Ok(())
}

fn scan_keep_dir(plan: &mut Plan, root: &Path, dir: &Path, category: Category) -> Result<()> {
    acc_mut(plan, category).present = true;
    let mut specials = Vec::new();
    visit_tree(
        dir,
        |_, meta| add_file(acc_mut(plan, category), meta),
        |path, _| specials.push(path),
    )?;
    for path in specials {
        let meta = fs::symlink_metadata(&path)?;
        push_unknown(plan, root, &path, &meta, false)?;
    }
    Ok(())
}

fn visit_tree<F, S>(dir: &Path, mut on_file: F, mut on_special: S) -> Result<()>
where
    F: FnMut(&Path, &fs::Metadata),
    S: FnMut(PathBuf, EntryKind),
{
    visit_tree_inner(dir, &mut on_file, &mut on_special)
}

fn visit_tree_inner<F, S>(dir: &Path, on_file: &mut F, on_special: &mut S) -> Result<()>
where
    F: FnMut(&Path, &fs::Metadata),
    S: FnMut(PathBuf, EntryKind),
{
    for entry in read_sorted(dir)? {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        match entry_kind(&meta) {
            EntryKind::Directory => visit_tree_inner(&path, on_file, on_special)?,
            EntryKind::File => on_file(&path, &meta),
            kind => on_special(path, kind),
        }
    }
    Ok(())
}

fn push_unknown(
    plan: &mut Plan,
    root: &Path,
    path: &Path,
    meta: &fs::Metadata,
    measure_tree: bool,
) -> Result<()> {
    let mut measured = Measured::default();
    match entry_kind(meta) {
        EntryKind::Directory if measure_tree => {
            visit_tree(
                path,
                |_, file_meta| add_measured(&mut measured, file_meta),
                |_, _| {},
            )?;
        }
        EntryKind::File => add_measured(&mut measured, meta),
        _ => {}
    }
    let files = if measure_tree && meta.is_dir() {
        measured.files
    } else {
        measured.files.max(1)
    };
    plan.unknowns.push(DisplayRow {
        category: "未识别".to_string(),
        path: relative_path(root, path),
        files,
        bytes: measured.bytes,
        modified: format_modified(measured.latest),
        action: "未识别",
    });
    Ok(())
}

fn rows_from(plan: &Plan) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    for (category, acc) in &plan.categories {
        if !acc.present {
            continue;
        }
        rows.push(DisplayRow {
            category: category.label().to_string(),
            path: category.path_label().to_string(),
            files: acc.files,
            bytes: acc.bytes,
            modified: format_modified(acc.latest),
            action: category.action_label(),
        });
    }
    rows.extend(plan.unknowns.clone());
    rows
}

fn render_plan(rows: &[DisplayRow]) -> String {
    let mut out = render_table(rows);
    let total = rows.iter().map(|row| row.bytes).sum();
    let deletable = rows
        .iter()
        .filter(|row| row.action == "可删除")
        .map(|row| row.bytes)
        .sum();
    out.push_str(&format!("合计 {}\n", format_byte_size(total)));
    out.push_str(&format!("其中可删除 {}\n", format_byte_size(deletable)));
    out
}

fn render_table(rows: &[DisplayRow]) -> String {
    let headers = ["类别", "路径", "文件数", "大小", "最近修改", "动作"];
    let body: Vec<[String; 6]> = rows
        .iter()
        .map(|row| {
            [
                row.category.clone(),
                row.path.clone(),
                row.files.to_string(),
                format_byte_size(row.bytes),
                row.modified.clone(),
                row.action.to_string(),
            ]
        })
        .collect();
    let mut widths = headers.map(display_width);
    for cells in &body {
        for (index, cell) in cells.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
    }
    let mut out = String::new();
    push_border(&mut out, &widths, Border::Top);
    push_cells(&mut out, &headers.map(str::to_string), &widths);
    push_border(&mut out, &widths, Border::Middle);
    for cells in &body {
        push_cells(&mut out, cells, &widths);
    }
    push_border(&mut out, &widths, Border::Bottom);
    out
}

#[derive(Clone, Copy)]
enum Border {
    Top,
    Middle,
    Bottom,
}

fn push_border(out: &mut String, widths: &[usize; 6], kind: Border) {
    let (left, mid, right, fill) = match kind {
        Border::Top => ('┌', '┬', '┐', '─'),
        Border::Middle => ('├', '┼', '┤', '─'),
        Border::Bottom => ('└', '┴', '┘', '─'),
    };
    out.push(left);
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            out.push(mid);
        }
        for _ in 0..(width + 2) {
            out.push(fill);
        }
    }
    out.push(right);
    out.push('\n');
}

fn push_cells(out: &mut String, cells: &[String; 6], widths: &[usize; 6]) {
    out.push('│');
    for (cell, width) in cells.iter().zip(widths.iter()) {
        out.push(' ');
        out.push_str(cell);
        let padding = width.saturating_sub(display_width(cell));
        for _ in 0..padding {
            out.push(' ');
        }
        out.push(' ');
        out.push('│');
    }
    out.push('\n');
}

fn apply_deletes(root: &Path, plan: &Plan) -> DeleteReport {
    let mut report = DeleteReport::default();
    for path in &plan.delete_files {
        match delete_file_if_safe(root, path) {
            Ok(Some(bytes)) => {
                report.files += 1;
                report.bytes += bytes;
            }
            Ok(None) => {}
            Err(err) => report.failures.push(format!("{}: {err}", path.display())),
        }
    }
    let mut dirs = plan.baseline_dirs.clone();
    dirs.sort_by(|left, right| {
        right
            .components()
            .count()
            .cmp(&left.components().count())
            .then_with(|| right.cmp(left))
    });
    for dir in dirs {
        match remove_dir_if_empty(root, &dir) {
            Ok(true) => report.directories += 1,
            Ok(false) => {}
            Err(err) => report.failures.push(format!("{}: {err}", dir.display())),
        }
    }
    report
}

fn delete_file_if_safe(root: &Path, path: &Path) -> Result<Option<u64>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if !meta.file_type().is_file() {
        return Ok(None);
    }
    if !is_within_root(root, path) {
        return Ok(None);
    }
    let bytes = meta.len();
    fs::remove_file(path)?;
    Ok(Some(bytes))
}

fn remove_dir_if_empty(root: &Path, dir: &Path) -> Result<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    let mut entries = fs::read_dir(dir)?;
    if entries.next().is_some() {
        return Ok(false);
    }
    if !is_within_root(root, dir) {
        return Ok(false);
    }
    fs::remove_dir(dir)?;
    Ok(true)
}

fn is_within_root(root: &Path, path: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    path.starts_with(root)
}

fn read_sorted(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn entry_kind(meta: &fs::Metadata) -> EntryKind {
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        EntryKind::Symlink
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn acc_mut(plan: &mut Plan, category: Category) -> &mut Acc {
    plan.categories.entry(category).or_default()
}

fn add_file(acc: &mut Acc, meta: &fs::Metadata) {
    acc.present = true;
    add_measured_parts(&mut acc.files, &mut acc.bytes, &mut acc.latest, meta);
}

fn add_measured(measured: &mut Measured, meta: &fs::Metadata) {
    add_measured_parts(
        &mut measured.files,
        &mut measured.bytes,
        &mut measured.latest,
        meta,
    );
}

fn add_measured_parts(
    files: &mut usize,
    bytes: &mut u64,
    latest: &mut Option<SystemTime>,
    meta: &fs::Metadata,
) {
    *files += 1;
    *bytes = bytes.saturating_add(meta.len());
    if let Ok(modified) = meta.modified()
        && latest.is_none_or(|current| modified > current)
    {
        *latest = Some(modified);
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn format_byte_size(bytes: u64) -> String {
    let gigabytes = (bytes as f64) / 1_000_000_000.0;
    format!("{bytes} 字节 / {gigabytes:.2} GB")
}

fn format_modified(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "-".to_string();
    };
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

impl Category {
    fn label(self) -> &'static str {
        match self {
            Self::LiveDb => "活库",
            Self::LiveSidecar => "活库伴随文件",
            Self::Migration => "迁移副本",
            Self::ConfigBackup => "配置备份",
            Self::Baseline => "研究基线",
            Self::Logs => "日志",
            Self::Cache => "缓存",
            Self::CodexTracer => "Codex tracer",
            Self::Bin => "包装脚本",
            Self::Exports => "导出",
            Self::Lock => "锁",
            Self::Pricing => "价表",
        }
    }

    fn path_label(self) -> &'static str {
        match self {
            Self::LiveDb => "llmusage.db",
            Self::LiveSidecar => "llmusage.db-wal / llmusage.db-shm",
            Self::Migration => "backups/llmusage.db.pre-*",
            Self::ConfigBackup => "backups/*.bak",
            Self::Baseline => "baselines/",
            Self::Logs => "logs/",
            Self::Cache => "cache/",
            Self::CodexTracer => "codex-tracer.db",
            Self::Bin => "bin/",
            Self::Exports => "exports/",
            Self::Lock => "worker.lock",
            Self::Pricing => "pricing/",
        }
    }

    fn action_label(self) -> &'static str {
        match action_for(self) {
            Action::Delete => "可删除",
            Action::Keep => "保留",
            Action::Unknown => "未识别",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::paths::AppPaths;

    #[test]
    fn config_backup_wins_over_migration_prefix() {
        let (class, action) = classify_entry(
            "llmusage.db.pre-custom.bak",
            EntryKind::File,
            EntryLocation::BackupsChild,
        );
        assert_eq!(class, Class::Known(Category::ConfigBackup));
        assert_eq!(action, Action::Keep);
    }

    #[test]
    fn migration_copies_and_sidecars_are_deletable() {
        for name in [
            "llmusage.db.pre-0.5.0",
            "llmusage.db.pre-0.23-host",
            "llmusage.db.pre-accounting-v2-lossy-20260716-144952.sqlite",
            "llmusage.db.pre-schema-v18-20260729-010307.sqlite-wal",
            "llmusage.db.pre-schema-v18-20260729-010307.sqlite-shm",
        ] {
            let (class, action) =
                classify_entry(name, EntryKind::File, EntryLocation::BackupsChild);
            assert_eq!(class, Class::Known(Category::Migration), "{name}");
            assert_eq!(action, Action::Delete, "{name}");
        }
    }

    #[test]
    fn symlink_kind_is_never_deletable() {
        for (name, location) in [
            ("baselines", EntryLocation::Root),
            ("llmusage.db.pre-0.5.0", EntryLocation::BackupsChild),
            ("snapshot.db", EntryLocation::BaselinesChild),
        ] {
            let (class, action) = classify_entry(name, EntryKind::Symlink, location);
            assert_eq!(class, Class::Unknown, "{name}");
            assert_eq!(action, Action::Unknown, "{name}");
        }
    }

    #[test]
    fn protected_root_files_are_kept() {
        for (name, category) in [
            ("llmusage.db", Category::LiveDb),
            ("llmusage.db-wal", Category::LiveSidecar),
            ("llmusage.db-shm", Category::LiveSidecar),
            ("codex-tracer.db", Category::CodexTracer),
            ("worker.lock", Category::Lock),
            ("pricing-cache-litellm.json", Category::Pricing),
        ] {
            let (class, action) = classify_entry(name, EntryKind::File, EntryLocation::Root);
            assert_eq!(class, Class::Known(category), "{name}");
            assert_eq!(action, Action::Keep, "{name}");
        }
        let (class, action) = classify_entry(
            "codex_notify_original.json",
            EntryKind::File,
            EntryLocation::BackupsChild,
        );
        assert_eq!(class, Class::Known(Category::ConfigBackup));
        assert_eq!(action, Action::Keep);
    }

    #[test]
    fn formats_decimal_gigabytes() {
        assert_eq!(format_byte_size(1_596_936_192), "1596936192 字节 / 1.60 GB");
        assert_eq!(format_byte_size(4_846_220_884), "4846220884 字节 / 4.85 GB");
        assert_eq!(format_byte_size(1_160_123_446), "1160123446 字节 / 1.16 GB");
    }

    #[test]
    fn renders_inventory_scale_sizes() {
        let text = render_table(&[
            row("活库", "llmusage.db", 1, 1_596_936_192, "保留"),
            row(
                "迁移副本",
                "backups/llmusage.db.pre-*",
                6,
                4_846_220_884,
                "可删除",
            ),
            row("研究基线", "baselines/", 3, 1_160_123_446, "可删除"),
        ]);
        assert!(text.contains("1.60 GB"), "{text}");
        assert!(text.contains("4.85 GB"), "{text}");
        assert!(text.contains("1.16 GB"), "{text}");
        assert!(text.contains("类别"));
        assert!(text.contains("动作"));
    }

    #[test]
    fn missing_root_is_empty_and_not_created() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let root = temp.path().join("missing-home");
        let outcome = clean_home(&root, true).expect("clean missing home");
        assert!(outcome.text.contains("没有可清理内容"));
        assert!(outcome.error.is_none());
        assert!(!root.exists());
    }

    #[test]
    fn dry_run_prints_categories_without_changing_bytes() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        write_sample(temp.path());
        let before = list_files(temp.path());
        let outcome = clean_home(temp.path(), false).expect("dry run");
        assert_eq!(before, list_files(temp.path()));
        assert!(outcome.error.is_none());
        assert!(outcome.text.contains("活库"), "{}", outcome.text);
        assert!(outcome.text.contains("迁移副本"), "{}", outcome.text);
        assert!(outcome.text.contains("研究基线"), "{}", outcome.text);
        assert!(outcome.text.contains("未修改磁盘"), "{}", outcome.text);
        assert!(!outcome.text.contains("已删除"), "{}", outcome.text);
    }

    #[test]
    fn yes_deletes_only_the_confirmed_set() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        write_sample(temp.path());
        let outcome = clean_home(temp.path(), true).expect("clean --yes");
        assert!(outcome.error.is_none(), "{}", outcome.text);
        assert!(outcome.text.contains("已删除"), "{}", outcome.text);
        assert!(temp.path().join("llmusage.db").is_file());
        assert!(temp.path().join("llmusage.db-wal").is_file());
        assert!(temp.path().join("llmusage.db-shm").is_file());
        assert!(temp.path().join("codex-tracer.db").is_file());
        assert!(temp.path().join("cache/subscription-usage.json").is_file());
        assert!(temp.path().join("logs/llmusage.ndjson").is_file());
        assert!(temp.path().join("worker.lock").is_file());
        assert!(temp.path().join("pricing/catalog.json").is_file());
        assert!(temp.path().join("unknown.txt").is_file());
        assert!(temp.path().join("backups/settings.json.bak").is_file());
        assert!(
            temp.path()
                .join("backups/llmusage.db.pre-custom.bak")
                .is_file()
        );
        assert!(
            temp.path()
                .join("backups/codex_notify_original.json")
                .is_file()
        );
        assert!(
            temp.path()
                .join("backups/nested/llmusage.db.pre-hidden")
                .is_file()
        );
        assert!(!temp.path().join("backups/llmusage.db.pre-0.5.0").exists());
        assert!(
            !temp
                .path()
                .join("backups/llmusage.db.pre-0.23-host")
                .exists()
        );
        assert!(
            !temp
                .path()
                .join("backups/llmusage.db.pre-accounting-v2.sqlite")
                .exists()
        );
        assert!(
            !temp
                .path()
                .join("backups/llmusage.db.pre-schema-v18.sqlite-wal")
                .exists()
        );
        assert!(!temp.path().join("baselines").exists());
    }

    #[test]
    fn canonical_path_outside_root_is_not_a_failed_delete() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let root = temp.path().join("home");
        fs::create_dir(&root).expect("root");
        let outside = temp.path().join("outside.bin");
        fs::write(&outside, b"abc").expect("outside file");
        let outside_dir = temp.path().join("outside-dir");
        fs::create_dir(&outside_dir).expect("outside dir");

        assert_eq!(
            delete_file_if_safe(&root, &outside).expect("skip file"),
            None
        );
        assert!(outside.is_file());
        assert!(!remove_dir_if_empty(&root, &outside_dir).expect("skip dir"));
        assert!(outside_dir.is_dir());
    }

    #[tokio::test]
    async fn command_uses_the_app_runtime_root() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        write_sample(temp.path());
        let app = AppContext {
            paths: AppPaths::with_root(temp.path().to_path_buf()).expect("paths"),
            current_exe: PathBuf::from("llmusage-test"),
        };
        run(&app, false).await.expect("clean command");
        assert!(temp.path().join("backups/llmusage.db.pre-0.5.0").is_file());
    }

    fn row(
        category: &str,
        path: &str,
        files: usize,
        bytes: u64,
        action: &'static str,
    ) -> DisplayRow {
        DisplayRow {
            category: category.to_string(),
            path: path.to_string(),
            files,
            bytes,
            modified: "-".to_string(),
            action,
        }
    }

    fn write_sample(root: &Path) {
        for relative in [
            "llmusage.db",
            "llmusage.db-wal",
            "llmusage.db-shm",
            "codex-tracer.db",
            "cache/subscription-usage.json",
            "logs/llmusage.ndjson",
            "worker.lock",
            "pricing/catalog.json",
            "bin/llmusage-hook.cmd",
            "exports/report.html",
            "unknown.txt",
            "backups/llmusage.db.pre-0.5.0",
            "backups/llmusage.db.pre-0.23-host",
            "backups/llmusage.db.pre-accounting-v2.sqlite",
            "backups/llmusage.db.pre-schema-v18.sqlite-wal",
            "backups/settings.json.bak",
            "backups/llmusage.db.pre-custom.bak",
            "backups/codex_notify_original.json",
            "backups/nested/llmusage.db.pre-hidden",
            "baselines/08-23/llmusage.db.pre-omp-split",
            "baselines/08-23/nested/aggregate_baseline.json",
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
            fs::write(&path, relative).expect("write sample");
        }
    }

    fn list_files(root: &Path) -> Vec<(String, u64)> {
        let mut files = Vec::new();
        collect_files(root, root, &mut files);
        files.sort();
        files
    }

    fn collect_files(root: &Path, dir: &Path, files: &mut Vec<(String, u64)>) {
        let mut entries = fs::read_dir(dir).expect("read dir").collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.as_ref().ok().map(fs::DirEntry::file_name));
        for entry in entries {
            let entry = entry.expect("entry");
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).expect("metadata");
            if meta.file_type().is_symlink() {
                files.push((relative_path(root, &path), 0));
                continue;
            }
            if meta.is_dir() {
                collect_files(root, &path, files);
            } else if meta.is_file() {
                files.push((relative_path(root, &path), meta.len()));
            }
        }
    }
}
