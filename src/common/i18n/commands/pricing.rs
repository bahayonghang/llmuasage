//! Pricing validation copy; the domain only carries neutral facts.
pub(crate) use crate::domain::pricing_catalog::diagnostic::DiagnosticFacts;
use crate::{common::i18n::Language, domain::pricing_catalog::diagnostic::LoadReason};

impl DiagnosticFacts {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::SnapshotKind {} => "pricing.snapshot_kind",
            Self::OverlayKind {} => "pricing.overlay_kind",
            Self::CompileKind {} => "pricing.compile_kind",
            Self::UnknownRemoval { .. } => "pricing.unknown_removal",
            Self::CompiledMatcher { .. } => "pricing.compiled_matcher",
            Self::RootObject {} => "pricing.root_object",
            Self::NoLiteRows {} => "pricing.no_lite_rows",
            Self::LegacyMatcher { .. } => "pricing.legacy_matcher",
            Self::Schema { .. } => "pricing.schema",
            Self::EmptyVersion {} => "pricing.empty_version",
            Self::BaseRemovals {} => "pricing.base_removals",
            Self::DuplicateModel { .. } => "pricing.duplicate_model",
            Self::NoSources { .. } => "pricing.no_sources",
            Self::DuplicateSource { .. } => "pricing.duplicate_source",
            Self::NoMatchers { .. } => "pricing.no_matchers",
            Self::EmptyMatcher { .. } => "pricing.empty_matcher",
            Self::DuplicateMatcher { .. } => "pricing.duplicate_matcher",
            Self::TierName { .. } => "pricing.tier_name",
            Self::TierThreshold { .. } => "pricing.tier_threshold",
            Self::ContextWindow { .. } => "pricing.context_window",
            Self::DuplicateRemoval { .. } => "pricing.duplicate_removal",
            Self::Identifier { .. } => "pricing.identifier",
            Self::InvalidRate { .. } => "pricing.invalid_rate",
            Self::Disagree { .. } => "pricing.disagree",
            Self::MarkerEncode { .. } => "pricing.marker_encode",
            Self::MetadataIncomplete { .. } => "pricing.metadata_incomplete",
            Self::Load { .. } => "pricing.load",
            Self::RefreshIdentity { .. } => "pricing.refresh_identity",
            Self::MarkerParse { .. } => "pricing.marker_parse",
            Self::MarkerMissingIdentity {} => "pricing.marker_missing_identity",
            Self::MarkerEmpty {} => "pricing.marker_empty",
            Self::DigestCollision { .. } => "pricing.digest_collision",
            Self::MetadataMismatch { .. } => "pricing.metadata_mismatch",
            Self::OverlayNotAddressed { .. } => "pricing.overlay_not_addressed",
            Self::UnsafePath { .. } => "pricing.unsafe_path",
            Self::LocalOnly { .. } => "pricing.local_only",
            Self::NotFile { .. } => "pricing.not_file",
            Self::InvalidFilename { .. } => "pricing.invalid_filename",
            Self::DigestMismatch { .. } => "pricing.digest_mismatch",
        }
    }

    pub(crate) fn render(&self, language: Language) -> Option<String> {
        use Language::{Chinese, English};
        let text = match self {
            Self::SnapshotKind {} => language
                .select(
                    "pricing snapshot must have kind `base`",
                    "价格快照的 kind 必须为 `base`",
                )
                .into(),
            Self::OverlayKind {} => language
                .select(
                    "pricing overlay must have kind `overlay`",
                    "价格覆盖文件的 kind 必须为 `overlay`",
                )
                .into(),
            Self::CompileKind {} => language
                .select(
                    "only a base catalog can be compiled",
                    "只能编译基础价格目录",
                )
                .into(),
            Self::UnknownRemoval { model } => match language {
                English => format!("pricing overlay removes unknown model id `{model}`"),
                Chinese => format!("价格覆盖文件要移除的模型 id `{model}` 不存在"),
            },
            Self::CompiledMatcher {
                version,
                mode,
                source,
                matcher,
            } => match language {
                English => format!(
                    "pricing catalog {version} has duplicate {mode} matcher for source '{source}': '{matcher}'"
                ),
                Chinese => format!(
                    "价格目录 {version} 中来源 '{source}' 的 {mode} 匹配规则重复：'{matcher}'"
                ),
            },
            Self::RootObject {} => language
                .select(
                    "pricing snapshot must be a JSON object",
                    "价格快照必须是 JSON 对象",
                )
                .into(),
            Self::NoLiteRows {} => language
                .select(
                    "pricing snapshot did not contain any LiteLLM rows with input_cost_per_token",
                    "价格快照未包含带有 input_cost_per_token 的 LiteLLM 记录",
                )
                .into(),
            Self::LegacyMatcher { index } => match language {
                English => format!("legacy pricing entry {index} has no matcher"),
                Chinese => format!("旧版价格条目 {index} 缺少匹配规则"),
            },
            Self::Schema { actual, expected } => match language {
                English => format!(
                    "unsupported pricing catalog schema_version {actual}; expected {expected}"
                ),
                Chinese => format!("不支持价格目录 schema_version {actual}；需要 {expected}"),
            },
            Self::EmptyVersion {} => language
                .select(
                    "pricing catalog version must not be empty",
                    "价格目录版本不能为空",
                )
                .into(),
            Self::BaseRemovals {} => language
                .select(
                    "base pricing catalog cannot remove models",
                    "基础价格目录不能移除模型",
                )
                .into(),
            Self::DuplicateModel { model } => match language {
                English => format!("pricing catalog has duplicate model id `{model}`"),
                Chinese => format!("价格目录的模型 id `{model}` 重复"),
            },
            Self::NoSources { model } => match language {
                English => format!("pricing model `{model}` must declare at least one source"),
                Chinese => format!("价格模型 `{model}` 必须声明至少一个来源"),
            },
            Self::DuplicateSource { model, source } => match language {
                English => format!("pricing model `{model}` has duplicate source `{source}`"),
                Chinese => format!("价格模型 `{model}` 的来源 `{source}` 重复"),
            },
            Self::NoMatchers { model } => match language {
                English => format!("pricing model `{model}` must declare at least one matcher"),
                Chinese => format!("价格模型 `{model}` 必须声明至少一个匹配规则"),
            },
            Self::EmptyMatcher { model } => match language {
                English => format!("pricing model `{model}` contains an empty matcher"),
                Chinese => format!("价格模型 `{model}` 包含空匹配规则"),
            },
            Self::DuplicateMatcher {
                model,
                mode,
                matcher,
            } => match language {
                English => format!(
                    "pricing model `{model}` contains a duplicate {mode} matcher `{matcher}`"
                ),
                Chinese => format!("价格模型 `{model}` 的 {mode} 匹配规则 `{matcher}` 重复"),
            },
            Self::TierName { model } => match language {
                English => format!("pricing model `{model}` has an empty or duplicate tier name"),
                Chinese => format!("价格模型 `{model}` 的分档名称为空或重复"),
            },
            Self::TierThreshold { model } => match language {
                English => format!(
                    "pricing model `{model}` tiers must use strictly increasing positive thresholds"
                ),
                Chinese => format!("价格模型 `{model}` 的分档阈值必须为严格递增的正数"),
            },
            Self::ContextWindow { model } => match language {
                English => format!("pricing model `{model}` context_window must be positive"),
                Chinese => format!("价格模型 `{model}` 的 context_window 必须为正数"),
            },
            Self::DuplicateRemoval { model } => match language {
                English => format!("pricing overlay removes model `{model}` more than once"),
                Chinese => format!("价格覆盖文件重复移除模型 `{model}`"),
            },
            Self::Identifier { label, value } => {
                let zh_label = match label.as_str() {
                    "model id" => "模型 id",
                    "source" => "来源",
                    "removed model id" => "要移除的模型 id",
                    _ => label,
                };
                match language {
                    English => format!(
                        "pricing catalog {label} `{value}` must use lowercase ASCII letters, digits, dot, dash, or underscore"
                    ),
                    Chinese => format!(
                        "价格目录的{zh_label} `{value}` 只能使用 ASCII 小写字母、数字、点、连字符或下划线"
                    ),
                }
            }
            Self::InvalidRate {
                model,
                tier,
                channel,
            } => match language {
                English => format!("pricing model `{model}` tier `{tier}` has invalid {channel}"),
                Chinese => format!("价格模型 `{model}` 的分档 `{tier}` 中 {channel} 无效"),
            },
            Self::Disagree { model } => match language {
                English => format!("pricing entries with id `{model}` disagree across sources"),
                Chinese => format!("不同来源中 id 为 `{model}` 的价格条目不一致"),
            },
            Self::MarkerEncode { detail } => match language {
                English => {
                    format!("pricing recompute in-progress marker could not be encoded: {detail}")
                }
                Chinese => format!("无法编码价格重算进行中标记；原始详情：{detail}"),
            },
            Self::MetadataIncomplete { layer } => match language {
                English => format!("active pricing {layer} metadata is incomplete"),
                Chinese => format!("当前价格 {layer} 元数据不完整"),
            },
            Self::Load {
                in_progress,
                identity,
                reason,
            } => {
                let reason = match reason.as_ref() {
                    LoadReason::Known { facts } => facts.render(language)?,
                    LoadReason::Original { detail } => format!(
                        "{}: {detail}",
                        language.select("Original detail", "原始详情")
                    ),
                };
                match language {
                    English => format!(
                        "{} `{identity}` could not be loaded: {reason}",
                        if *in_progress {
                            "pricing recompute in-progress catalog"
                        } else {
                            "active pricing catalog"
                        }
                    ),
                    Chinese => format!(
                        "无法加载{} `{identity}`：{reason}",
                        if *in_progress {
                            "价格重算进行中目录"
                        } else {
                            "当前价格目录"
                        }
                    ),
                }
            }
            Self::RefreshIdentity { identity } => match language {
                English => format!(
                    "refreshed pricing catalog identity `{identity}` is not a public catalog"
                ),
                Chinese => format!("刷新的价格目录标识 `{identity}` 不是公共价格目录"),
            },
            Self::MarkerParse { detail } => match language {
                English => format!("pricing recompute in-progress marker is invalid: {detail}"),
                Chinese => format!("价格重算进行中标记无效；原始详情：{detail}"),
            },
            Self::MarkerMissingIdentity {} => language
                .select(
                    "pricing recompute in-progress marker is missing catalog identity",
                    "价格重算进行中标记缺少目录标识",
                )
                .into(),
            Self::MarkerEmpty {} => language
                .select(
                    "pricing recompute in-progress marker is empty",
                    "价格重算进行中标记为空",
                )
                .into(),
            Self::DigestCollision { path } => match language {
                English => format!("pricing catalog digest collision at {path}"),
                Chinese => format!("价格目录摘要冲突：{path}"),
            },
            Self::MetadataMismatch {
                overlay,
                identity,
                declared,
            } => {
                let label = if *overlay {
                    language.select("pricing overlay", "价格覆盖文件")
                } else {
                    language.select("pricing catalog", "价格目录")
                };
                match language {
                    English => format!(
                        "{label} metadata expects `{identity}` but file declares `{declared}`"
                    ),
                    Chinese => format!("{label}元数据要求 `{identity}`，但文件声明 `{declared}`"),
                }
            }
            Self::OverlayNotAddressed { identity } => match language {
                English => format!("pricing overlay `{identity}` is not content-addressed"),
                Chinese => format!("价格覆盖文件 `{identity}` 未使用内容寻址"),
            },
            Self::UnsafePath { path } => match language {
                English => format!("pricing catalog file must be a safe relative path: {path}"),
                Chinese => format!("价格目录文件必须使用安全的相对路径：{path}"),
            },
            Self::LocalOnly { snapshot } => {
                let label = if *snapshot {
                    language.select("pricing snapshot", "价格快照")
                } else {
                    language.select("pricing overlay", "价格覆盖文件")
                };
                match language {
                    English => format!("{label} must be a local file; URLs are not supported"),
                    Chinese => format!("{label}必须是本地文件；不支持 URL"),
                }
            }
            Self::NotFile { snapshot, path } => {
                let label = if *snapshot {
                    language.select("pricing snapshot", "价格快照")
                } else {
                    language.select("pricing overlay", "价格覆盖文件")
                };
                match language {
                    English => format!("{label} path does not exist or is not a file: {path}"),
                    Chinese => format!("{label}路径不存在或不是文件：{path}"),
                }
            }
            Self::InvalidFilename { path } => match language {
                English => format!("invalid content-addressed pricing filename `{path}`"),
                Chinese => format!("内容寻址价格文件名无效：`{path}`"),
            },
            Self::DigestMismatch {
                path,
                expected,
                actual,
            } => match language {
                English => format!(
                    "pricing catalog digest mismatch for `{path}`: expected {expected}, got {actual}"
                ),
                Chinese => {
                    format!("价格目录 `{path}` 摘要不匹配：需要 {expected}，实际为 {actual}")
                }
            },
        };
        Some(text)
    }
}

impl From<DiagnosticFacts> for super::DiagnosticFacts {
    fn from(facts: DiagnosticFacts) -> Self {
        Self::Pricing(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_pricing_metadata_only_hides_original_error_when_complete() {
        let facts = super::super::DiagnosticFacts::Pricing(DiagnosticFacts::Load {
            in_progress: false,
            identity: "user-version".into(),
            reason: Box::new(LoadReason::Known {
                facts: DiagnosticFacts::NoLiteRows {},
            }),
        });
        let encoded = serde_json::to_value(&facts).unwrap();
        for language in [Language::English, Language::Chinese] {
            let fields = serde_json::json!({"event_code":"commands.mod.03", "error":"EXACT_CANONICAL", "diagnostic_code":facts.code(), "diagnostic_facts":encoded.to_string()});
            let valid = crate::common::i18n::logging::project(
                language,
                "ERROR",
                Some("run failed"),
                &fields,
                false,
            );
            assert!(
                valid.contains(language.select("LiteLLM rows", "LiteLLM 记录")),
                "{valid}"
            );
            assert!(!valid.contains("EXACT_CANONICAL"), "{valid}");
            let mut future = encoded.clone();
            future["facts"]["reason"]["facts"]["future_extra"] =
                serde_json::json!("extra diagnostic");
            let mut fields = fields;
            fields["diagnostic_facts"] = serde_json::json!(future.to_string());
            for detailed in [false, true] {
                let fallback = crate::common::i18n::logging::project(
                    language,
                    "ERROR",
                    Some("run failed"),
                    &fields,
                    detailed,
                );
                assert!(
                    fallback.contains("EXACT_CANONICAL") && fallback.contains("future_extra"),
                    "{fallback}"
                );
            }
        }
    }
}
