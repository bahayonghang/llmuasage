//! Shared pure sync presentation. Data and SyncEvent serialization remain language-neutral.
pub use super::{Language, current, text};

use crate::{models::SourceKind, parsers::SyncEvent};

fn source_label(source: SourceKind) -> &'static str {
    crate::registry::source_descriptor(source)
        .map_or(source.as_str(), |descriptor| descriptor.display_name)
}

fn version_label(version: &str) -> String {
    if version.chars().count() > 20 {
        format!("{}…", version.chars().take(19).collect::<String>())
    } else {
        version.into()
    }
}

/// One permanent line replacing the per-migration `Schema vN … completed` lines.
pub(crate) fn migration_summary(
    count: u32,
    latest_version: u32,
    elapsed_ms: u64,
    language: Language,
) -> String {
    if language == Language::Chinese {
        format!("数据库 schema 已升级到 v{latest_version}（{count} 个迁移，{elapsed_ms}ms）")
    } else {
        let plural = if count == 1 { "" } else { "s" };
        format!(
            "Database schema upgraded to v{latest_version} ({count} migration{plural}, {elapsed_ms}ms)"
        )
    }
}

pub(crate) fn progress(event: &SyncEvent, language: Language) -> Option<String> {
    let zh = language == Language::Chinese;
    Some(match event {
        SyncEvent::BootstrapStarted => language
            .select(
                "Checking database schema and pricing catalog...",
                "检查数据库 schema 与定价目录...",
            )
            .into(),
        SyncEvent::MigrationStarted {
            version,
            name,
            latest_version,
        } => {
            if zh {
                format!("升级数据库 schema v0 → v{latest_version}，正在执行 v{version} {name}...")
            } else {
                format!("Upgrading schema v0 → v{latest_version}; running v{version} {name}...")
            }
        }
        SyncEvent::MigrationFinished {
            version,
            name,
            elapsed_ms,
        } => {
            if zh {
                format!("数据库 schema v{version} {name} 完成（{elapsed_ms}ms）")
            } else {
                format!("Schema v{version} {name} completed ({elapsed_ms}ms)")
            }
        }
        SyncEvent::PricingUpgradeStarted {
            from_version,
            to_version,
            total_events,
        } => {
            let from_version = version_label(from_version);
            let to_version = version_label(to_version);
            if zh {
                format!("升级定价目录 {from_version} → {to_version}：共 {total_events} 条事件...")
            } else {
                format!("Updating pricing {from_version} → {to_version}: {total_events} events...")
            }
        }
        SyncEvent::PricingUpgradeProgress {
            from_version,
            to_version,
            processed_events,
            total_events,
            elapsed_ms,
        } => {
            let from_version = version_label(from_version);
            let to_version = version_label(to_version);
            let percentage = if *total_events == 0 {
                100.0
            } else {
                *processed_events as f64 * 100.0 / *total_events as f64
            };
            if zh {
                format!(
                    "升级定价目录 {from_version} → {to_version}：已处理 {processed_events}/{total_events} 条（{percentage:.1}%），已用 {elapsed_ms}ms"
                )
            } else {
                format!(
                    "Updating pricing {from_version} → {to_version}: {processed_events}/{total_events} events ({percentage:.1}%), elapsed {elapsed_ms}ms"
                )
            }
        }
        SyncEvent::PricingBucketReconcileStarted {
            to_version,
            bucket_count,
        } => {
            let to_version = version_label(to_version);
            if zh {
                format!("事件定价完成，正在对账 {bucket_count} 个 {to_version} 汇总桶...")
            } else {
                format!(
                    "Event pricing finished; reconciling {bucket_count} {to_version} summary buckets..."
                )
            }
        }
        SyncEvent::PricingUpgradeFinished {
            to_version,
            updated_events,
            bucket_count,
            deleted_orphan_buckets,
            elapsed_ms,
            ..
        } => {
            let to_version = version_label(to_version);
            if zh {
                format!(
                    "定价目录 {to_version} 升级完成：{updated_events} 条事件，{bucket_count} 个桶，清理 {deleted_orphan_buckets} 个孤立桶（{elapsed_ms}ms）"
                )
            } else {
                format!(
                    "Pricing {to_version} updated: {updated_events} events, {bucket_count} buckets, {deleted_orphan_buckets} orphan buckets removed ({elapsed_ms}ms)"
                )
            }
        }
        SyncEvent::LockWaiting { .. } => language
            .select(
                "Waiting for the SQLite sync worker lock...",
                "等待 SQLite sync worker 锁...",
            )
            .into(),
        SyncEvent::LockAcquired { wait_ms } => {
            if zh {
                format!("已获取 SQLite sync worker 锁（等待 {wait_ms}ms）")
            } else {
                format!("SQLite sync worker lock acquired (waited {wait_ms}ms)")
            }
        }
        SyncEvent::TokenAccountingRepairStarted { sources } => {
            let sources = sources
                .iter()
                .map(|source| source.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if zh {
                format!(
                    "警告：检测到旧版 token accounting（{sources}）；本轮将自动执行无损重建，不会自动启用 --allow-lossy-rebuild。"
                )
            } else {
                format!(
                    "Warning: legacy token accounting detected ({sources}); this repair is lossless and does not enable --allow-lossy-rebuild."
                )
            }
        }
        SyncEvent::TokenAccountingRepairFinished { sources } => {
            let sources = sources
                .iter()
                .map(|source| source.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if zh {
                format!("Token accounting 自动修复完成：{sources}")
            } else {
                format!("Token accounting repair completed: {sources}")
            }
        }
        SyncEvent::SourceStarted {
            source,
            files_total,
        } => {
            let name = source_label(*source);
            if matches!(source, SourceKind::Codex | SourceKind::Claude) {
                if zh {
                    format!("{name}: 本轮重放 {files_total} 个文件...")
                } else {
                    format!("{name}: replaying {files_total} files for this run...")
                }
            } else if zh {
                format!("{name}: 扫描 {files_total} 个文件...")
            } else {
                format!("{name}: scanning {files_total} files...")
            }
        }
        SyncEvent::Progress {
            source,
            files_scanned,
            records_imported,
            ..
        } => {
            let name = source_label(*source);
            if matches!(source, SourceKind::Codex | SourceKind::Claude) {
                if zh {
                    format!("{name}: 已重放 {files_scanned} 个文件，导入 {records_imported} 条")
                } else {
                    format!(
                        "{name}: replayed {files_scanned} files, imported {records_imported} records"
                    )
                }
            } else if zh {
                format!("{name}: 已处理 {files_scanned}，导入 {records_imported} 条")
            } else {
                format!("{name}: processed {files_scanned}, imported {records_imported} records")
            }
        }
        SyncEvent::RemoteHostStarted { label, .. } => {
            if zh {
                format!("正在导入远端主机 {label}...")
            } else {
                format!("Importing remote host {label}...")
            }
        }
        SyncEvent::RemoteHostFinished { label, stats, .. } => {
            if zh {
                format!("远端主机 {label} 导入完成（{} 个来源）", stats.len())
            } else {
                format!("Remote host {label} imported ({} sources)", stats.len())
            }
        }
        SyncEvent::RemoteHostSkipped { label, reason, .. } => {
            if zh {
                format!("警告：跳过远端主机 {label}：{reason}")
            } else {
                format!("Warning: skipped remote host {label}: {reason}")
            }
        }
        SyncEvent::Failed { error } => {
            if zh {
                format!("同步失败：{error}")
            } else {
                format!("Sync failed: {error}")
            }
        }
        SyncEvent::Cancelled => language.select("Sync cancelled", "同步已取消").into(),
        SyncEvent::SourceFinished { .. }
        | SyncEvent::Started { .. }
        | SyncEvent::Finished { .. }
        | SyncEvent::RecentReady { .. } => return None,
    })
}

pub(crate) fn replay_message(
    source: SourceKind,
    pos: u64,
    len: u64,
    records: u64,
    language: Language,
) -> String {
    let name = source_label(source);
    match (language, len > 0 && pos == len) {
        (Language::Chinese, true) => format!("{name}: 重放完成，正在提交... · 已导入 {records} 条"),
        (Language::English, true) => {
            format!("{name}: replay complete, committing... · imported {records} records")
        }
        (Language::Chinese, false) => format!("{name}: 重放 {pos}/{len} · 导入 {records} 条"),
        (Language::English, false) => {
            format!("{name}: replay {pos}/{len} · imported {records} records")
        }
    }
}

pub(crate) fn failure(kind: &crate::sync::failure::FailureKind, language: Language) -> String {
    use crate::sync::failure::FailureKind;
    match kind {
        FailureKind::LossyRebuild { risks } => {
            let facts = risks.iter().map(|risk| format!("{}@{}: {}={} {}={}", risk.source, super::logging::safe_text(&risk.host_id),
                language.select("missing_files", "缺失文件"), risk.missing_files,
                language.select("protected_events", "受保护事件"), risk.protected_events)).collect::<Vec<_>>().join("; ");
            if language == Language::Chinese {
                format!("拒绝有损重建：导入历史对应的来源文件缺失（{facts}）。普通 `llmusage sync` 只标记缺失文件，不会删除历史；`llmusage sync --rebuild` 会先删除可重建的历史，无法恢复原文件已缺失的记录。请恢复来源文件，或使用 --allow-lossy-rebuild 明确接受清除无法重建的历史。")
            } else {
                format!("Refusing lossy sync --rebuild because imported usage has missing source files ({facts}). Regular `llmusage sync` marks missing files without deleting history. Rebuilding deletes rebuildable usage first and cannot recover records whose original files are gone. Restore source files or pass --allow-lossy-rebuild to explicitly accept clearing unrebuildable history.")
            }
        }
        FailureKind::OmpBeforePiSplit => language.select(
            "Refusing --source omp because stored pi rows use the pre-split token-accounting contract. Run `llmusage sync --rebuild --source pi` first, then retry `llmusage sync --source omp`.",
            "拒绝同步 --source omp：已存储的 pi 记录仍使用拆分前的 token 计数规则。请先运行 `llmusage sync --rebuild --source pi` 迁移记录，再重试 `llmusage sync --source omp`。"
        ).into(),
        FailureKind::UnsupportedRebuild { source } => if language == Language::Chinese {
            format!("无法重建来源 `{source}`：没有被动解析器；已保留历史用量。")
        } else {
            format!("Cannot rebuild source `{source}` because it has no passive parser; historical usage was preserved.")
        },
    }
}

/// Known importer facts come from this call; arbitrary transport detail stays visible.
pub(crate) fn remote_detail(
    detail: &str,
    facts: Option<&crate::remote::diagnostic::DiagnosticFacts>,
    language: Language,
) -> String {
    facts
        .map(|facts| facts.render(language))
        .unwrap_or_else(|| {
            format!(
                "{}: {detail}",
                language.select("Original detail", "原始详情")
            )
        })
}

pub(crate) fn progress_with_remote(
    event: &SyncEvent,
    facts: Option<&crate::remote::diagnostic::DiagnosticFacts>,
    language: Language,
) -> Option<String> {
    if let SyncEvent::RemoteHostSkipped { label, reason, .. } = event {
        let detail = remote_detail(reason, facts, language);
        return Some(match language {
            Language::English => format!("Warning: skipped remote host {label}: {detail}"),
            Language::Chinese => format!("警告：跳过远端主机 {label}：{detail}"),
        });
    }
    progress(event, language)
}

/// Details have already been projected by the receiving UI on its render thread.
pub(crate) fn registry_failure(
    command: &str,
    status: &str,
    finish_detail: &str,
    original_detail: Option<&str>,
    language: Language,
) -> String {
    let original = original_detail.unwrap_or_else(|| language.select("none", "无"));
    match language {
        Language::English => format!(
            "Could not finish {command} run log: {finish_detail}; original status={status}; original error={original}"
        ),
        Language::Chinese => format!(
            "记录 {command} 的运行日志收尾失败：{finish_detail}；原始终态={status}；原始错误={original}"
        ),
    }
}

/// Pure paired notice content shared by CLI diagnostics and the TUI observer.
pub(crate) fn notice_lines(
    notice: &crate::sync::notices::RunNotice,
    language: Language,
) -> Vec<String> {
    use crate::sync::notices::RunNotice;
    let text = |english, chinese| language.select(english, chinese);
    let zh = language == Language::Chinese;
    let line = match notice {
        RunNotice::LegacySkipped { source, retained } => if zh { format!("  保留历史：{retained} 条事件；跳过旧版 token 计数来源。修复：`llmusage sync --rebuild --source {source}`。") } else { format!("  History kept: {retained} events; legacy token accounting source skipped. Repair: `llmusage sync --rebuild --source {source}`.") },
        RunNotice::RecoveryOffered { coverage, .. } => if zh { format!("  已提供恢复选择：保留的历史 {} 条事件。", coverage.stored_events) } else { format!("  Recovery choice offered for {} preserved events.", coverage.stored_events) },
        RunNotice::RecoveryChoice { accepted, .. } => match (zh, accepted) {
            (true, true) => "  已明确接受有损重建；下列库存数不代表精确丢失数。".into(),
            (false, true) => "  History-loss rebuild explicitly accepted; resulting stock is not an exact loss count.".into(),
            (true, false) => "  已选择保留历史，本轮不执行有损重建。".into(),
            (false, false) => "  History kept; no lossy rebuild in this run.".into(),
        },
        RunNotice::Coverage { source, coverage, interactive, windowed, kept } => {
            let mut lines = vec![if zh { format!("  保留历史：{} 条事件；当前输入 {}，新增 {}。", coverage.stored_events, coverage.discovered_count, coverage.new_files_count) } else { format!("  History kept: {} events; current inputs {}, new {}.", coverage.stored_events, coverage.discovered_count, coverage.new_files_count) }];
            if coverage.missing_count > 0 { lines.push(format!("  {}: {}", text("Tracked databases missing from disk", "磁盘上缺失的已跟踪数据库"), coverage.missing_count)); }
            if coverage.out_of_scope_count > 0 { lines.push(format!("  {}: {}", text("Tracked inputs still present outside discovery coverage", "仍存在但超出发现范围的已跟踪输入"), coverage.out_of_scope_count)); }
            if coverage.unreadable_count > 0 { lines.push(format!("  {}: {}", text("Unreadable tracked databases", "不可读的已跟踪数据库"), coverage.unreadable_count)); }
            if coverage.discovery_incomplete { lines.push(format!("  {}", text("Discovery incomplete", "来源发现不完整"))); }
            if coverage.unreadable_count > 0 || coverage.discovery_incomplete {
                lines.push(format!("  {}", text("Group writes skipped; restore access and retry sync. Loss acceptance cannot bypass this protection.", "已跳过该组写入；请恢复访问后重试同步。接受数据丢失不能绕过此保护。")));
            } else {
                lines.push(format!("  {}", text("Rebuilding discards missing/out-of-scope history and retains only currently parseable inputs.", "重建会丢弃缺失或范围外输入的历史，只保留当前可解析输入的重建结果。")));
                if *windowed { lines.push(format!("  {}", text("Run sync without --recent-days to choose recovery interactively.", "去掉 --recent-days 后运行同步，可交互选择恢复方案。"))); }
                else if !interactive && !kept { lines.push(format!("  {}", text("Run sync in a terminal to choose recovery; this noninteractive run kept history.", "在终端中运行同步以选择恢复方案；本轮非交互同步已保留历史。"))); }
                lines.push(if zh { format!("  先恢复输入，或明确接受历史丢失：`llmusage sync --rebuild --source {source} --allow-lossy-rebuild`。") } else { format!("  Restore inputs, or explicitly accept history loss: `llmusage sync --rebuild --source {source} --allow-lossy-rebuild`.") });
            }
            return lines;
        },
        RunNotice::RetainedHistory { retained, .. } => if zh { format!("  旧版历史已保留；当前来源库存 {retained} 条事件。") } else { format!("  Previous-version history retained; current source stock is {retained} events.") },
        RunNotice::SourceIssue { source, issue } => {
            use crate::domain::source_diagnostics::SourceIssueCode::*;
            let (en, cn) = match issue.code {
                DiscoveryIncomplete => ("Discovery incomplete; restore source access", "来源发现不完整；请恢复访问"),
                FingerprintUnavailable => ("Fingerprint unavailable; restore database access", "无法取得指纹；请恢复数据库访问"),
                TrackedMemberMissing => ("Tracked database missing; restore inputs or explicitly accept history loss", "已跟踪数据库缺失；请恢复输入或明确接受历史丢失"),
                TrackedMemberOutOfScope => ("Tracked input still exists outside discovery coverage", "已跟踪输入仍存在，但超出发现范围"),
                TrackedMemberUnreadable => ("Tracked database unreadable; restore access", "已跟踪数据库不可读；请恢复访问"),
                SnapshotChanged => ("Snapshot changed; retry after source writes finish", "快照已变化；请等待来源写入结束后重试"),
                MetadataUnreadable => ("Metadata unreadable; restore a complete readable database", "元数据不可读；请恢复完整可读的数据库"),
                IncompleteSnapshot => ("Snapshot incomplete; restore complete inputs", "快照不完整；请恢复完整输入"),
            };
            if zh { format!("  故障 [{}]：{}；数量 {}；来源 {source}。", issue.code.as_str(), cn, issue.count) } else { format!("  Fault [{}]: {en}; count {}; source {source}.", issue.code.as_str(), issue.count) }
        },
        RunNotice::SourceFailed { failure, .. } => match failure {
            crate::sync::notices::SourceFailure::SqliteOpen => format!("  {}", text("Cannot open source SQLite database; history kept. Restore readable input and retry sync.", "无法打开来源 SQLite 数据库；已保留历史。请恢复可读输入后重试同步。")),
            crate::sync::notices::SourceFailure::Inventory { diagnostic } => {
                let mut lines = vec![if zh { format!("  来源库存发现故障：{} 条；展示 {}/{}；另有 {} 条未展示。", diagnostic.total, diagnostic.samples.len(), diagnostic.total, diagnostic.omitted) } else { format!("  Source inventory faults: {}; shown {}/{}; {} not shown.", diagnostic.total, diagnostic.samples.len(), diagnostic.total, diagnostic.omitted) }];
                for sample in &diagnostic.samples {
                    use crate::parsers::feedback::InventoryIssue;
                    let line = match sample {
                        InventoryIssue::RootNotDirectory => text("Source root is not a directory", "来源根路径不是目录").into(),
                        InventoryIssue::ExternalIo { detail } => format!("{}: {}", text("Cannot enumerate source inputs; original OS detail", "无法枚举来源输入；原始系统详情"), crate::common::i18n::logging::safe_text(detail)),
                        InventoryIssue::Traversal => text("Cannot traverse source directory", "无法遍历来源目录").into(),
                        InventoryIssue::Unknown { detail } => format!("{}: {}", text("Original detail", "原始详情"), crate::common::i18n::logging::safe_text(detail)),
                    };
                    lines.push(format!("    {line}"));
                }
                return lines;
            },
            crate::sync::notices::SourceFailure::Other { detail } => format!("  {}: {}", text("Source failed; original detail", "来源失败；原始详情"), crate::common::i18n::logging::safe_text(detail)),
        },
        RunNotice::RemoteSkipped { label, detail, facts, .. } => format!("{} · {}: {}", text("Remote host skipped", "远端主机已跳过"), crate::common::i18n::logging::safe_text(label), crate::common::i18n::logging::safe_text(&crate::common::i18n::sync::remote_detail(detail, facts.as_deref(), language))),
        RunNotice::Cancelled => text("Sync cancelled; completed commits retain their existing semantics.", "同步已取消；已完成提交保留原有语义。").into(),
    };
    vec![line]
}

pub(crate) fn notice(
    notice: &crate::sync::notices::RunNotice,
    language: Language,
) -> Option<String> {
    let detail = notice_lines(notice, language)
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ");
    Some(match notice.source() {
        Some(source) => format!("{source}: {detail}"),
        None => detail,
    })
}

#[cfg(test)]
mod notice_tests {
    use super::*;
    use crate::sync::notices::{RunNotice, SourceFailure};

    #[test]
    fn neutral_notice_projection_pairs_sources_counts_actions_and_original_details() {
        let coverage = crate::parsers::antigravity::AntigravityProductCoverage {
            source: SourceKind::Antigravity,
            discovered_count: 3,
            new_files_count: 2,
            tracked_count: 8,
            stored_events: 21,
            missing_count: 1,
            out_of_scope_count: 2,
            unreadable_count: 1,
            discovery_incomplete: true,
        };
        let cases = vec![
            (
                RunNotice::LegacySkipped {
                    source: SourceKind::Codex,
                    retained: 21,
                },
                "History kept",
                "保留历史",
            ),
            (
                RunNotice::RecoveryOffered {
                    source: coverage.source,
                    coverage: coverage.clone(),
                },
                "Recovery choice",
                "恢复选择",
            ),
            (
                RunNotice::RecoveryChoice {
                    source: coverage.source,
                    accepted: true,
                },
                "explicitly accepted",
                "明确接受",
            ),
            (
                RunNotice::RecoveryChoice {
                    source: coverage.source,
                    accepted: false,
                },
                "History kept",
                "保留历史",
            ),
            (
                RunNotice::Coverage {
                    source: coverage.source,
                    coverage: coverage.clone(),
                    interactive: false,
                    windowed: false,
                    kept: true,
                },
                "Loss acceptance cannot bypass",
                "不能绕过此保护",
            ),
            (
                RunNotice::RetainedHistory {
                    source: SourceKind::Codex,
                    retained: 21,
                },
                "history retained",
                "历史已保留",
            ),
            (
                RunNotice::SourceFailed {
                    source: SourceKind::Zcode,
                    failure: SourceFailure::SqliteOpen,
                },
                "Cannot open source SQLite",
                "无法打开来源 SQLite",
            ),
            (
                RunNotice::SourceFailed {
                    source: SourceKind::Codex,
                    failure: SourceFailure::Other {
                        detail: "EXTERNAL_DETAIL".into(),
                    },
                },
                "original detail",
                "原始详情",
            ),
            (
                RunNotice::RemoteSkipped {
                    host_id: "devbox".into(),
                    label: "My host".into(),
                    detail: "EXTERNAL_DETAIL".into(),
                    facts: None,
                },
                "Original detail",
                "原始详情",
            ),
            (RunNotice::Cancelled, "Sync cancelled", "同步已取消"),
        ];
        for (fact, english, chinese) in cases {
            let en = notice(&fact, Language::English).unwrap();
            let zh = notice(&fact, Language::Chinese).unwrap();
            assert!(en.contains(english), "{en}");
            assert!(zh.contains(chinese), "{zh}");
            if let Some(source) = fact.source() {
                assert!(en.starts_with(source.as_str()) && zh.starts_with(source.as_str()));
            }
        }
        use crate::domain::source_diagnostics::{SourceIssue, SourceIssueCode, SourceIssueScope};
        for code in [
            SourceIssueCode::DiscoveryIncomplete,
            SourceIssueCode::FingerprintUnavailable,
            SourceIssueCode::TrackedMemberMissing,
            SourceIssueCode::TrackedMemberOutOfScope,
            SourceIssueCode::TrackedMemberUnreadable,
            SourceIssueCode::SnapshotChanged,
            SourceIssueCode::MetadataUnreadable,
            SourceIssueCode::IncompleteSnapshot,
        ] {
            let fact = RunNotice::SourceIssue {
                source: SourceKind::Antigravity,
                issue: SourceIssue {
                    code,
                    count: 14,
                    observed_at: chrono::Utc::now(),
                    scope: SourceIssueScope::ProductGroup,
                },
            };
            for language in [Language::English, Language::Chinese] {
                let rendered = notice(&fact, language).unwrap();
                assert!(rendered.contains(code.as_str()) && rendered.contains("14"));
                assert!(rendered.contains(language.select("Fault", "故障")));
                assert!(!rendered.contains("observed_at"));
            }
        }
        let inventory = RunNotice::SourceFailed {
            source: SourceKind::Codex,
            failure: SourceFailure::Inventory {
                diagnostic: crate::parsers::feedback::InventoryDiagnostic {
                    samples: vec![
                        crate::parsers::feedback::InventoryIssue::RootNotDirectory,
                        crate::parsers::feedback::InventoryIssue::Traversal,
                        crate::parsers::feedback::InventoryIssue::Unknown {
                            detail: "EXTERNAL_DETAIL".into(),
                        },
                    ],
                    total: 14,
                    omitted: 11,
                },
            },
        };
        for language in [Language::English, Language::Chinese] {
            let lines = notice_lines(&inventory, language);
            assert_eq!(lines.len(), 4);
            assert!(lines[0].contains("3/14") && lines[0].contains("11"));
            assert!(lines[3].contains("EXTERNAL_DETAIL"));
            assert!(lines[3].contains(language.select("Original detail", "原始详情")));
        }
    }
}
