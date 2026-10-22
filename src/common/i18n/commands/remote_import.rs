//! Human remote-import presentation; importers carry only neutral facts.
use crate::common::i18n::Language;
use crate::remote::SHARD_PROTOCOL_VERSION;
pub(crate) use crate::remote::diagnostic::DiagnosticFacts;

impl DiagnosticFacts {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Protocol { .. } => "remote.import.protocol",
            Self::FirstRecord {} => "remote.import.first_record",
            Self::BeforeHeader { .. } => "remote.import.before_header",
            Self::NoHeader { .. } => "remote.import.no_header",
            Self::NoTrailer { .. } => "remote.import.no_trailer",
            Self::ExitWithoutTrailer { .. } => "remote.import.exit_without_trailer",
            Self::MissingVersions { .. } => "remote.import.missing_versions",
            Self::UnknownSource { .. } => "remote.import.unknown_source",
            Self::Accounting { .. } => "remote.import.accounting",
            Self::Historical { .. } => "remote.import.historical",
            Self::Unlisted { .. } => "remote.import.unlisted",
            Self::SkippedLines { .. } => "remote.import.skipped_lines",
        }
    }

    pub(crate) fn render(&self, language: Language) -> String {
        use Language::{Chinese, English};
        match self {
            Self::Protocol { remote, schema } => match language {
                English => format!(
                    "shard protocol mismatch: local={SHARD_PROTOCOL_VERSION} remote={remote} (remote schema_version={schema}); upgrade llmusage so both sides share shard_protocol {SHARD_PROTOCOL_VERSION}"
                ),
                Chinese => format!(
                    "分片协议不一致：本地={SHARD_PROTOCOL_VERSION} 远程={remote}（远程 schema_version={schema}）；请升级 llmusage，使两端使用 shard_protocol {SHARD_PROTOCOL_VERSION}"
                ),
            },
            Self::FirstRecord {} => language
                .select(
                    "first deserialized shard record must be a header",
                    "首条反序列化的分片记录必须是 header",
                )
                .into(),
            Self::BeforeHeader { host } => match language {
                English => format!("remote host {host} emitted a shard before a validated header"),
                Chinese => format!("远程主机 {host} 在 header 验证完成前发送了分片"),
            },
            Self::NoHeader { host, stderr } => match language {
                English => {
                    format!("remote shard stream from host {host} had no header record: {stderr}")
                }
                Chinese => format!("主机 {host} 的远程分片流没有 header 记录；原始详情：{stderr}"),
            },
            Self::NoTrailer { host, stderr } => {
                let suffix = original_suffix(stderr, language);
                match language {
                    English => format!(
                        "remote shard stream from host {host} ended without a trailer; imported events were kept but the host watermark was not advanced{suffix}"
                    ),
                    Chinese => format!(
                        "主机 {host} 的远程分片流结束时缺少 trailer；已保留导入的事件，但未推进主机水位{suffix}"
                    ),
                }
            }
            Self::ExitWithoutTrailer { status, stderr } => match language {
                English => {
                    format!("remote sync --emit-shards exited {status} without a trailer: {stderr}")
                }
                Chinese => format!(
                    "远程 sync --emit-shards 以状态 {status} 退出且没有 trailer；原始详情：{stderr}"
                ),
            },
            Self::MissingVersions { host } => match language {
                English => format!(
                    "remote host {host} shard header is missing source token-accounting versions; upgrade llmusage on that host so both sides share shard_protocol {SHARD_PROTOCOL_VERSION} and the header lists each source's token_accounting_version"
                ),
                Chinese => format!(
                    "远程主机 {host} 的分片 header 缺少来源 token 计数版本；请升级该主机上的 llmusage，使两端使用 shard_protocol {SHARD_PROTOCOL_VERSION}，并在 header 中列出每个来源的 token_accounting_version"
                ),
            },
            Self::UnknownSource { host, source } => match language {
                English => format!(
                    "remote host {host} shard header lists unknown source {source}; upgrade llmusage so both sides share shard_protocol {SHARD_PROTOCOL_VERSION}"
                ),
                Chinese => format!(
                    "远程主机 {host} 的分片 header 列出了未知来源 {source}；请升级 llmusage，使两端使用 shard_protocol {SHARD_PROTOCOL_VERSION}"
                ),
            },
            Self::Accounting {
                host,
                source,
                remote,
                expected,
            } => {
                let remote = remote.map_or_else(
                    || language.select("<missing>", "<缺失>").to_string(),
                    |value| value.to_string(),
                );
                let source = source.as_str();
                match language {
                    English => format!(
                        "token accounting mismatch for host {host} source {source}: local={expected} remote={remote}; upgrade llmusage on that host so source {source} reports token_accounting_version {expected} in shard protocol {SHARD_PROTOCOL_VERSION}"
                    ),
                    Chinese => format!(
                        "主机 {host} 来源 {source} 的 token 计数版本不一致：本地={expected} 远程={remote}；请升级该主机上的 llmusage，使来源 {source} 在分片协议 {SHARD_PROTOCOL_VERSION} 中报告 token_accounting_version {expected}"
                    ),
                }
            }
            Self::Historical {
                host,
                source,
                marker,
                expected,
            } => {
                let historical = marker.map_or_else(
                    || language.select("unknown", "未知").to_string(),
                    |value| value.to_string(),
                );
                let source = source.as_str();
                match language {
                    English => format!(
                        "remote host {host} source {source} already has imported rows without a trusted current token-accounting marker (historical={historical}, current={expected}); a full restore is required (not implemented); existing events and watermarks were left unchanged"
                    ),
                    Chinese => format!(
                        "远程主机 {host} 来源 {source} 已有导入记录，但没有可信的当前 token 计数标记（历史={historical}，当前={expected}）；需要完整恢复（尚未实现）；已有事件和水位保持不变"
                    ),
                }
            }
            Self::Unlisted { host, source } => {
                let source = source.as_str();
                match language {
                    English => format!(
                        "remote host {host} shard source {source} is not listed in the header source_accounting_versions map; upgrade llmusage on that host so the header lists every shard source and its token_accounting_version"
                    ),
                    Chinese => format!(
                        "远程主机 {host} 的分片来源 {source} 未列在 header 的 source_accounting_versions 映射中；请升级该主机上的 llmusage，使 header 列出每个分片来源及其 token_accounting_version"
                    ),
                }
            }
            Self::SkippedLines { host, lines } => match language {
                English => format!("skipped {lines} non-JSON shard lines from host {host}"),
                Chinese => format!("已跳过主机 {host} 的 {lines} 行非 JSON 分片内容"),
            },
        }
    }
}

fn original_suffix(detail: &str, language: Language) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!("{}{}", language.select(": ", "；原始详情："), detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::i18n::{commands, logging};
    use crate::models::SourceKind;

    #[test]
    fn remote_import_catalog_has_paired_strict_facts_and_visible_unknown_metadata() {
        let facts = [
            DiagnosticFacts::Protocol {
                remote: 99,
                schema: 42,
            },
            DiagnosticFacts::FirstRecord {},
            DiagnosticFacts::BeforeHeader {
                host: "devbox".into(),
            },
            DiagnosticFacts::NoHeader {
                host: "devbox".into(),
                stderr: "SSH_DETAIL".into(),
            },
            DiagnosticFacts::NoTrailer {
                host: "devbox".into(),
                stderr: "SSH_DETAIL".into(),
            },
            DiagnosticFacts::ExitWithoutTrailer {
                status: 9,
                stderr: "SSH_DETAIL".into(),
            },
            DiagnosticFacts::MissingVersions {
                host: "devbox".into(),
            },
            DiagnosticFacts::UnknownSource {
                host: "devbox".into(),
                source: "future".into(),
            },
            DiagnosticFacts::Accounting {
                host: "devbox".into(),
                source: SourceKind::Codex,
                remote: None,
                expected: 3,
            },
            DiagnosticFacts::Historical {
                host: "devbox".into(),
                source: SourceKind::Codex,
                marker: None,
                expected: 3,
            },
            DiagnosticFacts::Unlisted {
                host: "devbox".into(),
                source: SourceKind::Codex,
            },
            DiagnosticFacts::SkippedLines {
                host: "devbox".into(),
                lines: 1,
            },
        ];
        let mut codes = std::collections::BTreeSet::new();
        for facts in facts {
            assert!(codes.insert(facts.code()));
            assert_ne!(
                facts.render(Language::English),
                facts.render(Language::Chinese)
            );
            let facts = commands::DiagnosticFacts::from(facts);
            let raw = serde_json::to_value(&facts).unwrap();
            let roundtrip: commands::DiagnosticFacts = serde_json::from_value(raw.clone()).unwrap();
            assert_eq!(serde_json::to_value(roundtrip).unwrap(), raw);
            let mut unknown = raw.clone();
            unknown["facts"]["future_field"] = serde_json::json!("FUTURE_DETAIL");
            for language in [Language::English, Language::Chinese] {
                let fields = serde_json::json!({"event_code":"commands.mod.04", "error":"CANONICAL_DETAIL", "diagnostic_code":facts.code(), "diagnostic_facts":raw.to_string()});
                let output =
                    logging::project(language, "ERROR", Some("run failed"), &fields, false);
                assert!(
                    output.contains(&facts.render(language).unwrap()),
                    "{output}"
                );
                assert!(!output.contains("CANONICAL_DETAIL"), "{output}");
                let mut unknown_fields = fields;
                unknown_fields["diagnostic_facts"] = serde_json::json!(unknown.to_string());
                for detailed in [false, true] {
                    let output = logging::project(
                        language,
                        "ERROR",
                        Some("run failed"),
                        &unknown_fields,
                        detailed,
                    );
                    assert!(
                        output.contains("CANONICAL_DETAIL") && output.contains("FUTURE_DETAIL"),
                        "{output}"
                    );
                }
            }
        }
    }
}
