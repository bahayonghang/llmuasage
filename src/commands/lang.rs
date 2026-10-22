use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Context, Result};

use crate::{
    common::i18n::{Language, current},
    runtime::preferences,
};

use super::cli_errors::Diagnostic;

pub fn run(root: &std::path::Path, code: Option<&str>) -> Result<()> {
    let language = current();
    let choice = if let Some(code) = code {
        Some(Language::parse(code).ok_or_else(|| {
            Diagnostic::new(
                format!("Unknown language '{code}'; use en or zh."),
                format!("未知语言“{code}”；请使用 en 或 zh。"),
            )
        })?)
    } else {
        choose(
            language,
            io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal(),
            &mut io::stdin().lock(),
            &mut io::stdout().lock(),
        )?
    };
    if let Some(choice) = choice {
        preferences::write(root, choice).with_context(|| {
            Diagnostic::new(
                format!(
                    "Could not save CLI language to {}",
                    root.join(preferences::SETTINGS_FILE).display()
                ),
                format!(
                    "无法保存 CLI 语言到 {}",
                    root.join(preferences::SETTINGS_FILE).display()
                ),
            )
        })?;
        println!(
            "{} {}",
            choice.select("CLI language saved:", "已保存 CLI 语言："),
            choice.code()
        );
    }
    Ok(())
}

pub fn choose(
    language: Language,
    terminal: bool,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Option<Language>> {
    writeln!(
        output,
        "{} {}",
        language.select("Current CLI language:", "当前 CLI 语言："),
        language.code()
    )?;
    if !terminal {
        writeln!(
            output,
            "{}",
            language.select(
                "Set with `llmusage lang en` or `llmusage lang zh`.",
                "使用 `llmusage lang en` 或 `llmusage lang zh` 设置。"
            )
        )?;
        return Ok(None);
    }
    writeln!(output, "1. English\n2. 中文（简体）")?;
    loop {
        write!(
            output,
            "{} ",
            language.select(
                "Choose [1/2, Enter/q to cancel]:",
                "请选择 [1/2，回车/q 取消]："
            )
        )?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line).with_context(|| {
            Diagnostic::new("Could not read language selection", "无法读取语言选择")
        })? == 0
        {
            return Ok(None);
        }
        match line.trim() {
            "" | "q" | "Q" => return Ok(None),
            "1" | "en" => return Ok(Some(Language::English)),
            "2" | "zh" | "zh-CN" => return Ok(Some(Language::Chinese)),
            _ => writeln!(
                output,
                "{}",
                language.select(
                    "Choose 1 or 2, or q to cancel.",
                    "请输入 1 或 2，或 q 取消。"
                )
            )?,
        }
    }
}
