//! CLI-only recovery I/O. Coverage, choices and rebuild safety remain domain-owned.
use std::io::{BufRead, Write};

use crate::{
    common::i18n::Language, models::SourceKind, parsers::antigravity::AntigravityProductCoverage,
    sync::types::AntigravityRecoveryChoice,
};

pub(crate) fn question(
    language: Language,
    source: SourceKind,
    coverage: &AntigravityProductCoverage,
) -> String {
    let mut text = format!(
        "\n{} · {}\n  {}: {}\n  {}: {}\n",
        language.select("Recovery required", "需要选择恢复方案"),
        source.as_str(),
        language.select("Preserved events", "保留的历史事件"),
        coverage.stored_events,
        language.select("Current inputs", "当前发现的输入"),
        coverage.discovered_count
    );
    if coverage.missing_count > 0 {
        text.push_str(&format!(
            "  {}: {}\n",
            language.select(
                "Tracked databases missing from disk",
                "磁盘上缺失的已跟踪数据库"
            ),
            coverage.missing_count
        ));
    }
    if coverage.out_of_scope_count > 0 {
        text.push_str(&format!(
            "  {}: {}\n",
            language.select(
                "Tracked inputs still present outside current discovery coverage",
                "仍然存在但超出当前发现范围的已跟踪输入"
            ),
            coverage.out_of_scope_count
        ));
    }
    if coverage.unreadable_count > 0 {
        text.push_str(&format!(
            "  {}: {}\n",
            language.select("Unreadable inputs", "不可读输入"),
            coverage.unreadable_count
        ));
    }
    if coverage.discovery_incomplete {
        text.push_str(language.select("  Discovery is incomplete.\n", "  来源发现不完整。\n"));
    }
    text.push_str(&format!(
        "  {}: {}\n",
        language.select("New inputs", "新增输入"),
        coverage.new_files_count
    ));
    if coverage.unreadable_count > 0 || coverage.discovery_incomplete {
        text.push_str(language.select(
            "Restore access and retry sync; rebuilding cannot bypass this protection.\n",
            "请恢复访问后重试同步；重建不能绕过此保护。\n",
        ));
    } else {
        text.push_str(language.select("Rebuilding discards history from missing or out-of-scope inputs and keeps only currently parseable files.\nKeep history [k] / Rebuild and accept data loss [r] (default: keep): ", "重建会丢弃缺失或范围外输入对应的历史，只保留当前可解析文件的重建结果。\n保留历史 [k] / 重建并接受数据丢失 [r]（默认：保留）："));
    }
    text
}

pub(crate) fn prompt<R: BufRead, W: Write>(
    language: Language,
    reader: &mut R,
    writer: &mut W,
    source: SourceKind,
    coverage: &AntigravityProductCoverage,
) -> AntigravityRecoveryChoice {
    let _ = write!(writer, "{}", question(language, source, coverage));
    let _ = writer.flush();
    if !coverage.can_prompt_for_loss() {
        return AntigravityRecoveryChoice::Keep;
    }
    for attempt in 0..2 {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return AntigravityRecoveryChoice::Keep;
        }
        let value = line.trim();
        if value.eq_ignore_ascii_case("r") {
            return AntigravityRecoveryChoice::AcceptLoss;
        }
        if value.is_empty() || value.eq_ignore_ascii_case("k") {
            return AntigravityRecoveryChoice::Keep;
        }
        if attempt == 0 {
            let _ = write!(
                writer,
                "{}",
                language.select(
                    "Please enter 'k' to keep or 'r' to rebuild (default: keep): ",
                    "请输入 k 保留或 r 重建（默认：保留）："
                )
            );
            let _ = writer.flush();
        }
    }
    AntigravityRecoveryChoice::Keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read};

    fn coverage() -> AntigravityProductCoverage {
        AntigravityProductCoverage {
            source: SourceKind::AntigravityIde,
            discovered_count: 45,
            new_files_count: 12,
            tracked_count: 501,
            stored_events: 9591,
            missing_count: 2,
            out_of_scope_count: 3,
            unreadable_count: 0,
            discovery_incomplete: false,
        }
    }

    struct NeverRead;
    impl Read for NeverRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("blocked coverage must not read stdin")
        }
    }
    impl BufRead for NeverRead {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            panic!("blocked coverage must not read stdin")
        }
        fn consume(&mut self, _: usize) {}
    }
    struct ReadFailure;
    impl Read for ReadFailure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read denied"))
        }
    }
    impl BufRead for ReadFailure {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("read denied"))
        }
        fn consume(&mut self, _: usize) {}
    }

    #[test]
    fn recovery_choices_retry_and_safe_defaults_match_in_both_languages() {
        for language in [Language::English, Language::Chinese] {
            for (input, expected) in [
                ("k\n", AntigravityRecoveryChoice::Keep),
                ("K\n", AntigravityRecoveryChoice::Keep),
                ("r\n", AntigravityRecoveryChoice::AcceptLoss),
                ("R\n", AntigravityRecoveryChoice::AcceptLoss),
                ("\n", AntigravityRecoveryChoice::Keep),
                ("", AntigravityRecoveryChoice::Keep),
                ("bad\nr\n", AntigravityRecoveryChoice::AcceptLoss),
                ("bad\nbad\nr\n", AntigravityRecoveryChoice::Keep),
                ("bad\n", AntigravityRecoveryChoice::Keep),
            ] {
                let mut output = Vec::new();
                assert_eq!(
                    prompt(
                        language,
                        &mut input.as_bytes(),
                        &mut output,
                        SourceKind::AntigravityIde,
                        &coverage()
                    ),
                    expected,
                    "{language:?}: {input:?}"
                );
                let output = String::from_utf8(output).unwrap();
                for value in ["9591", "45", "12", "2", "3", "antigravity_ide"] {
                    assert!(output.contains(value), "{output}");
                }
                assert!(output.contains(language.select("missing from disk", "磁盘上缺失")));
                assert!(
                    output.contains(language.select("still present outside", "仍然存在但超出"))
                );
                assert_eq!(
                    output
                        .matches(language.select("Please enter", "请输入"))
                        .count(),
                    usize::from(input.starts_with("bad"))
                );
            }
            assert_eq!(
                prompt(
                    language,
                    &mut ReadFailure,
                    &mut Vec::new(),
                    SourceKind::AntigravityIde,
                    &coverage()
                ),
                AntigravityRecoveryChoice::Keep
            );
        }
    }

    #[test]
    fn unreadable_or_incomplete_discovery_never_offer_acceptance_or_read() {
        for language in [Language::English, Language::Chinese] {
            for unreadable in [false, true] {
                let mut coverage = coverage();
                coverage.unreadable_count = usize::from(unreadable);
                coverage.discovery_incomplete = !unreadable;
                let mut output = Vec::new();
                assert_eq!(
                    prompt(
                        language,
                        &mut NeverRead,
                        &mut output,
                        coverage.source,
                        &coverage
                    ),
                    AntigravityRecoveryChoice::Keep
                );
                let output = String::from_utf8(output).unwrap();
                assert!(!output.contains("[r]") && !output.contains("--allow-lossy-rebuild"));
                assert!(output.contains(language.select("Restore access", "恢复访问")));
            }
        }
    }
}
