use std::{
    ffi::OsString,
    fs,
    io::{self, BufRead, Read},
    path::Path,
    process::{Output, Stdio},
};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use llmusage::{
    commands::{self, cli_errors, entrypoint, help, lang},
    common::i18n::{self, Language},
    runtime::preferences,
};
use tempfile::TempDir;

fn invoke(root: &Path, args: &[&str]) -> Result<Output> {
    super::test_process::llmusage_command()
        .arg("--home")
        .arg(root)
        .args(args)
        .env_remove("LLMUSAGE_HOME")
        .env("LANG", "zh_CN.UTF-8")
        .env("LC_ALL", "zh_CN.UTF-8")
        .stdin(Stdio::null())
        .output()
        .context("spawn language test CLI")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn language_is_persistent_normalized_and_profile_isolated() -> Result<()> {
    let temp = TempDir::new()?;
    let a = temp.path().join("配置 A");
    let b = temp.path().join("B");
    let initial = invoke(&a, &["--help"])?;
    assert!(initial.status.success());
    assert!(stdout(&initial).contains("Common commands:"));
    assert!(!a.exists());
    let saved = invoke(&a, &["lang", "zh-CN"])?;
    assert!(saved.status.success(), "{saved:?}");
    assert!(stdout(&saved).contains("已保存 CLI 语言： zh"));
    assert_eq!(preferences::read(&a).language, Language::Chinese);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(
            a.join(preferences::SETTINGS_FILE)
        )?)?["language"],
        "zh"
    );
    assert!(stdout(&invoke(&a, &["--help"])?).contains("常用命令："));
    assert!(stdout(&invoke(&b, &["--help"])?).contains("Common commands:"));
    assert!(!b.exists());
    assert!(invoke(&a, &["lang", "en"])?.status.success());
    assert!(stdout(&invoke(&a, &["--help"])?).contains("Common commands:"));
    assert!(!a.join("logs").exists());
    assert!(!a.join("llmusage.db").exists());
    Ok(())
}

#[test]
fn help_version_parser_and_noninteractive_lang_never_create_root() -> Result<()> {
    let temp = TempDir::new()?;
    for args in [
        vec!["--help"],
        vec!["help"],
        vec!["help", "--zh"],
        vec!["help", "daily"],
        vec!["remote", "help", "add"],
        vec!["help", "remote", "handshake"],
        vec!["codex", "daily", "--help"],
        vec!["--version"],
        vec!["--unknown"],
        vec!["daily", "--since", "no"],
        vec!["remote", "add"],
        vec!["lang"],
    ] {
        let root = temp.path().join("absent");
        let output = invoke(&root, &args)?;
        assert!(!root.exists(), "{args:?}: {output:?}");
        if args.contains(&"--unknown") || args.contains(&"no") || args == ["remote", "add"] {
            assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        } else {
            assert!(output.status.success(), "{args:?}: {output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
    Ok(())
}

#[test]
fn nested_and_hidden_help_follow_preferences_and_preserve_exit_channels() -> Result<()> {
    let temp = TempDir::new()?;
    preferences::write(temp.path(), Language::Chinese)?;
    for args in [
        vec!["daily", "--help"],
        vec!["help", "daily"],
        vec!["remote", "handshake", "--help"],
        vec!["tui", "--help"],
        vec!["codex", "session", "--help"],
    ] {
        let output = invoke(temp.path(), &args)?;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(stdout(&output).contains("用法："), "{args:?}: {output:?}");
        assert!(!stdout(&output).contains("Usage:"));
    }
    for args in [
        vec!["daily", "--since", "20261340"],
        vec!["daily", "--timezone", "wrong"],
        vec!["daily", "--timezone", "中文"],
        vec!["daily", "--order", "wrong"],
        vec!["sync", "--emit-shards", "--rebuild"],
        vec!["logs", "--limit", "wrong"],
        vec!["remote", "add"],
        vec!["remote", "ad"],
        vec!["--unknown"],
    ] {
        let output = invoke(temp.path(), &args)?;
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains("错误:"), "{output:?}");
        assert!(!stderr(&output).contains("error:"));
    }
    assert!(!temp.path().join("logs").exists());
    assert!(!temp.path().join("llmusage.db").exists());
    Ok(())
}

#[test]
fn generated_help_commands_can_themselves_be_help_targets() -> Result<()> {
    let temp = TempDir::new()?;
    for language in [Language::English, Language::Chinese] {
        preferences::write(temp.path(), language)?;
        for (args, target) in [
            (vec!["help", "help"], "llmusage help —"),
            (vec!["help", "remote", "help"], "llmusage remote help —"),
            (vec!["remote", "help", "help"], "llmusage remote help —"),
        ] {
            let output = invoke(temp.path(), &args)?;
            assert!(output.status.success(), "{args:?}: {output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            assert!(stdout(&output).starts_with(target), "{args:?}: {output:?}");
            assert!(stdout(&output).contains(language.select("Usage:", "用法：")));
        }
    }
    assert!(!temp.path().join("logs").exists());
    Ok(())
}

#[test]
fn sync_validation_retains_invalid_input_and_code_in_both_languages() -> Result<()> {
    let temp = TempDir::new()?;
    for language in [Language::English, Language::Chinese] {
        preferences::write(temp.path(), language)?;
        for (args, code, value) in [
            (
                vec!["sync", "--source", "bad-source"],
                "unknown_source",
                "bad-source",
            ),
            (
                vec!["sync", "--recent-days", "4000"],
                "invalid_recent_days",
                "4000",
            ),
            (
                vec!["sync", "--parallelism", "40"],
                "invalid_parallelism",
                "40",
            ),
            (
                vec!["sync", "--emit-shards", "--source", "bad-source"],
                "unknown_source",
                "bad-source",
            ),
            (
                vec!["sync", "--emit-shards", "--parallelism", "40"],
                "invalid_parallelism",
                "40",
            ),
        ] {
            let output = invoke(temp.path(), &args)?;
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
            let error = stderr(&output);
            assert!(
                error.contains(code) && error.contains(value),
                "{args:?}: {error}"
            );
            assert_eq!(error.matches(language.select("Error:", "错误:")).count(), 1);
            if language == Language::Chinese {
                assert!(
                    !error.contains("unknown source:") && !error.contains("got 40"),
                    "{error}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn malformed_preferences_warn_once_without_overwriting_and_unknown_code_does_not_write()
-> Result<()> {
    let temp = TempDir::new()?;
    let settings = temp.path().join(preferences::SETTINGS_FILE);
    fs::write(&settings, b"{broken")?;
    let output = invoke(temp.path(), &["--help"])?;
    assert!(output.status.success());
    assert!(stdout(&output).contains("Common commands:"));
    assert_eq!(
        stderr(&output)
            .matches("Cannot read CLI language settings")
            .count(),
        1
    );
    assert_eq!(fs::read(&settings)?, b"{broken");
    let output = invoke(temp.path(), &["lang", "ja"])?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&settings)?, b"{broken");
    assert!(!stdout(&output).contains("saved"));
    assert!(invoke(temp.path(), &["lang", "zh"])?.status.success());
    assert_eq!(preferences::read(temp.path()).language, Language::Chinese);
    let absent = temp.path().join("absent");
    assert!(!invoke(&absent, &["lang", "ja"])?.status.success());
    assert!(!absent.exists());
    Ok(())
}

#[test]
fn forced_chinese_help_localizes_preference_repairs_without_writing() -> Result<()> {
    let temp = TempDir::new()?;
    let settings = temp.path().join(preferences::SETTINGS_FILE);
    for (document, expected) in [
        ("{broken", "原始详情"),
        ("[]", "配置必须是 JSON 对象"),
        (r#"{"language":12}"#, "语言必须是字符串"),
        (r#"{"language":"ja"}"#, "语言必须是 en 或 zh"),
    ] {
        fs::write(&settings, document)?;
        let output = invoke(temp.path(), &["help", "--zh"])?;
        assert!(output.status.success(), "{output:?}");
        assert!(stdout(&output).contains("常用命令："));
        let warning = stderr(&output);
        assert_eq!(
            warning.matches("无法读取 CLI 语言设置").count(),
            1,
            "{warning}"
        );
        assert!(warning.contains(expected), "{warning}");
        assert!(!warning.contains("Using English"), "{warning}");
        assert_eq!(fs::read_to_string(&settings)?, document);
        assert_eq!(fs::read_dir(temp.path())?.count(), 1);
    }
    Ok(())
}

#[test]
fn settings_writer_preserves_unknown_fields_and_recovers_before_reading() -> Result<()> {
    let temp = TempDir::new()?;
    let settings = temp.path().join(preferences::SETTINGS_FILE);
    fs::write(&settings, br#"{"language":"en","future":{"keep":42}}"#)?;
    preferences::write(temp.path(), Language::Chinese)?;
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&settings)?)?;
    assert_eq!(document["future"]["keep"], 42);
    fs::write(
        temp.path().join(".cli-settings.json.llmusage-recovery"),
        fs::read(&settings)?,
    )?;
    fs::write(
        temp.path().join(".cli-settings.json.llmusage-pending"),
        b"present\n",
    )?;
    fs::write(&settings, br#"{"language":"en"}"#)?;
    preferences::write(temp.path(), Language::English)?;
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&settings)?)?;
    assert_eq!(document["future"]["keep"], 42);
    assert_eq!(document["language"], "en");
    assert!(
        !temp
            .path()
            .join(".cli-settings.json.llmusage-pending")
            .exists()
    );
    Ok(())
}

#[test]
fn failed_settings_lock_keeps_old_document_and_reports_no_success() -> Result<()> {
    let temp = TempDir::new()?;
    let settings = temp.path().join(preferences::SETTINGS_FILE);
    let old = br#"{"language":"en","future":1}"#;
    fs::write(&settings, old)?;
    fs::create_dir(temp.path().join(preferences::LOCK_FILE))?;
    let output = invoke(temp.path(), &["lang", "zh"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("Could not save CLI language"));
    assert!(!stdout(&output).contains("已保存"));
    assert_eq!(fs::read(&settings)?, old);
    Ok(())
}

#[test]
fn concurrent_settings_writers_are_serialized_without_worker_lock() -> Result<()> {
    let temp = TempDir::new()?;
    preferences::write(temp.path(), Language::English)?;
    // A held settings lock blocks settings writers, but never readers or help.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.path().join(preferences::LOCK_FILE))?;
    fs4::FileExt::lock(&lock)?;
    let mut writer = super::test_process::llmusage_command()
        .arg("--home")
        .arg(temp.path())
        .args(["lang", "zh"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn locked settings writer")?;
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(writer.try_wait()?.is_none());
    assert_eq!(preferences::read(temp.path()).language, Language::English);
    assert!(invoke(temp.path(), &["--help"])?.status.success());
    fs4::FileExt::unlock(&lock)?;
    assert!(writer.wait_with_output()?.status.success());
    let mut children = Vec::new();
    for index in 0..8 {
        children.push(
            super::test_process::llmusage_command()
                .arg("--home")
                .arg(temp.path())
                .args(["lang", if index % 2 == 0 { "en" } else { "zh" }])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .context("spawn concurrent preference writer")?,
        );
    }
    for child in children {
        let output = child.wait_with_output()?;
        assert!(output.status.success(), "{output:?}");
    }
    preferences::write(temp.path(), Language::English)?;
    assert_eq!(preferences::read(temp.path()).language, Language::English);
    // A worker database lock is unrelated to the independent settings lock.
    let paths = llmusage::AppPaths::with_root(temp.path().to_path_buf())?;
    let store = llmusage::Store::new(&paths)?;
    assert!(!paths.db_path.exists());
    let _worker =
        store.acquire_worker_lock_with(std::time::Duration::ZERO, llmusage::HolderKind::Cli)?;
    assert!(invoke(temp.path(), &["lang", "zh"])?.status.success());
    Ok(())
}

struct NeverRead;
impl Read for NeverRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("non-TTY must not read stdin")
    }
}
impl BufRead for NeverRead {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("non-TTY must not read stdin")
    }
    fn consume(&mut self, _: usize) {
        panic!("non-TTY must not read stdin")
    }
}

#[test]
fn language_menu_never_reads_non_tty_and_cancels_empty_eof_or_q() -> Result<()> {
    let mut output = Vec::new();
    assert_eq!(
        lang::choose(Language::English, false, &mut NeverRead, &mut output)?,
        None
    );
    assert!(String::from_utf8(output)?.contains("llmusage lang zh"));
    for value in ["", "\n", "q\n", "Q\n"] {
        let mut output = Vec::new();
        assert_eq!(
            lang::choose(Language::Chinese, true, &mut value.as_bytes(), &mut output)?,
            None
        );
        let output = String::from_utf8(output)?;
        assert!(output.contains("English") && output.contains("中文（简体）"));
    }
    assert_eq!(
        lang::choose(
            Language::English,
            true,
            &mut "wrong\n2\n".as_bytes(),
            &mut Vec::new()
        )?,
        Some(Language::Chinese)
    );
    Ok(())
}

#[test]
fn bootstrap_selector_uses_schema_arity_and_short_attached_values() {
    let cases: &[(&[&str], Option<&str>, &[&str])] = &[
        (
            &["--home", "中文 空格", "daily"],
            Some("中文 空格"),
            &["daily"],
        ),
        (&["daily", "--home=A"], Some("A"), &["daily"]),
        (&["--home=A", "daily", "--home", "B"], Some("B"), &["daily"]),
        (&["--home=A", "--home=B", "daily"], Some("A"), &["daily"]),
        (
            &["--home=A", "remote", "--home=B", "list", "--home=C"],
            Some("C"),
            &["remote", "list"],
        ),
        (
            &["--project", "clean", "daily", "--home", "A"],
            Some("A"),
            &["daily"],
        ),
        (&["-pclean", "daily", "--home=A"], Some("A"), &["daily"]),
        (&["-jphelp", "daily", "--home=A"], Some("A"), &["daily"]),
        (
            &["--project=--home=fake", "daily", "--home=A"],
            Some("A"),
            &["daily"],
        ),
        (&["--project", "help", "clean"], None, &["clean"]),
        (&["--", "clean", "--home", "fake"], None, &[]),
        (
            &[
                "remote",
                "add",
                "clean",
                "help",
                "--command=--home=fake",
                "--home=A",
            ],
            Some("A"),
            &["remote", "add"],
        ),
        (&["--home"], None, &[]),
        (&["help", "remote", "add"], None, &["remote", "add"]),
        (&["help", "help"], None, &["help"]),
        (&["help", "remote", "help"], None, &["remote", "help"]),
        (
            &["--home=A", "help", "remote", "--home=B"],
            Some("A"),
            &["remote"],
        ),
        (&["--help", "daily", "--home=A"], Some("A"), &[]),
    ];
    let schema = entrypoint::schema();
    for (values, home, path) in cases {
        let args: Vec<OsString> = std::iter::once("llmusage")
            .chain(values.iter().copied())
            .map(Into::into)
            .collect();
        let selected = entrypoint::select(&schema, &args);
        assert_eq!(selected.home, home.map(Into::into), "{values:?}");
        assert_eq!(selected.path, *path, "{values:?}");
        let formal = commands::Cli::try_parse_from(&args);
        if let Ok(cli) = &formal {
            assert_eq!(
                selected.home, cli.home,
                "valid root selection must match formal Clap: {values:?}"
            );
        }
        let mut parser = schema.clone();
        let matches = parser.try_get_matches_from_mut(&args);
        assert_eq!(
            formal.as_ref().err().map(clap::Error::kind),
            matches.as_ref().err().map(clap::Error::kind)
        );
    }
}

#[test]
fn explicit_home_environment_home_and_clean_exception_are_isolated() -> Result<()> {
    let temp = TempDir::new()?;
    let user_home = temp.path().join("user");
    let default_root = user_home.join(".llmusage");
    let environment_root = temp.path().join("environment");
    let explicit_root = temp.path().join("explicit");
    fs::create_dir_all(&user_home)?;
    preferences::write(&environment_root, Language::Chinese)?;
    let output = super::test_process::llmusage_command()
        .arg("--help")
        .env("HOME", &user_home)
        .env("USERPROFILE", &user_home)
        .env("LLMUSAGE_HOME", &environment_root)
        .output()
        .context("spawn environment help")?;
    assert!(stdout(&output).contains("常用命令："));
    let output = super::test_process::llmusage_command()
        .args(["--help", "--home"])
        .arg(&explicit_root)
        .env("HOME", &user_home)
        .env("USERPROFILE", &user_home)
        .env("LLMUSAGE_HOME", &environment_root)
        .output()
        .context("spawn explicit help")?;
    assert!(stdout(&output).contains("Common commands:"));
    // The fake env home must not affect clean's settings or its no-create root.
    let output = super::test_process::llmusage_command()
        .args(["clean", "--help"])
        .env("HOME", &user_home)
        .env("USERPROFILE", &user_home)
        .env("LLMUSAGE_HOME", &environment_root)
        .output()
        .context("spawn clean help")?;
    assert!(stdout(&output).contains("Usage:"));
    assert!(!default_root.exists());
    assert!(!explicit_root.exists());
    Ok(())
}

#[test]
fn unreadable_settings_fall_back_without_creating_other_files() -> Result<()> {
    let temp = TempDir::new()?;
    fs::create_dir(temp.path().join(preferences::SETTINGS_FILE))?;
    let output = invoke(temp.path(), &["--help"])?;
    assert!(output.status.success());
    assert!(stdout(&output).contains("Common commands:"));
    assert_eq!(
        stderr(&output)
            .matches("Cannot read CLI language settings")
            .count(),
        1
    );
    assert_eq!(fs::read_dir(temp.path())?.count(), 1);
    Ok(())
}

#[cfg(windows)]
#[test]
fn denied_atomic_replace_preserves_old_settings_then_recovers() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = TempDir::new()?;
    preferences::write(temp.path(), Language::English)?;
    let path = temp.path().join(preferences::SETTINGS_FILE);
    let old = fs::read(&path)?;
    // Deny DELETE sharing so the actual ReplaceFileW fails with the old file intact.
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)?;
    let output = invoke(temp.path(), &["lang", "zh"])?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&path)?, old);
    assert!(!stdout(&output).contains("已保存"));
    drop(blocker);
    preferences::write(temp.path(), Language::Chinese)?;
    assert_eq!(preferences::read(temp.path()).language, Language::Chinese);
    assert!(
        !temp
            .path()
            .join(".cli-settings.json.llmusage-pending")
            .exists()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn bootstrap_home_preserves_non_unicode_os_paths() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let root = OsString::from_vec(b"path-\xff".to_vec());
    let mut attached = OsString::from("--home=");
    attached.push(&root);
    for args in [
        vec![
            "llmusage".into(),
            "--home".into(),
            root.clone(),
            "--help".into(),
        ],
        vec!["llmusage".into(), attached, "--help".into()],
    ] {
        let selection = entrypoint::select(&entrypoint::schema(), &args);
        assert_eq!(
            selection.home.unwrap().as_os_str().as_bytes(),
            root.as_os_str().as_bytes()
        );
    }
}

#[test]
fn every_command_and_argument_has_bilingual_help_and_supported_arity() {
    fn visit(
        root: &clap::Command,
        command: &clap::Command,
        path: &mut Vec<String>,
        count: &mut usize,
    ) {
        for language in [Language::English, Language::Chinese] {
            if !path.is_empty() {
                assert!(
                    help::command_description(path, language).is_some(),
                    "{path:?}"
                );
            }
            let rendered = help::command_help(root, path, language);
            assert!(!rendered.is_empty());
            for arg in command.get_arguments() {
                assert!(
                    help::argument_description(path, arg, language).is_some(),
                    "{path:?}: {}",
                    arg.get_id()
                );
                if arg.get_long().is_some() || arg.get_short().is_some() {
                    assert!(
                        arg.get_num_args().unwrap().max_values() <= 1,
                        "selector must be extended for {}",
                        arg.get_id()
                    );
                }
            }
        }
        *count += 1;
        for child in command.get_subcommands() {
            path.push(child.get_name().to_string());
            visit(root, child, path, count);
            path.pop();
        }
    }
    let root = entrypoint::schema();
    let mut count = 0;
    visit(&root, &root, &mut Vec::new(), &mut count);
    assert!(
        count >= 55,
        "all containers, focused leaves and hidden commands must be visited: {count}"
    );
    // Preserve the existing derive API for embedding callers.
    commands::Cli::command().debug_assert();
}

#[test]
fn parser_error_kinds_and_constraints_have_bilingual_projection() {
    use clap::error::ErrorKind::*;
    let kinds = [
        InvalidValue,
        UnknownArgument,
        InvalidSubcommand,
        NoEquals,
        ValueValidation,
        TooManyValues,
        TooFewValues,
        WrongNumberOfValues,
        ArgumentConflict,
        MissingRequiredArgument,
        MissingSubcommand,
        InvalidUtf8,
        DisplayHelp,
        DisplayHelpOnMissingArgumentOrSubcommand,
        DisplayVersion,
        Io,
        Format,
    ];
    for kind in kinds {
        assert_ne!(
            cli_errors::kind_message(kind, Language::English),
            cli_errors::kind_message(kind, Language::Chinese),
            "{kind:?}"
        );
        let error = clap::Error::raw(kind, "opaque English framework text");
        let output =
            cli_errors::parser_error(&error, &entrypoint::schema(), &[], Language::Chinese);
        assert!(output.contains(&format!("{kind:?}")));
        assert!(!output.contains("opaque English"));
    }
    for (args, expected) in [
        (&["daily", "--since", "bad"][..], "YYYY-MM-DD"),
        (&["daily", "--timezone", "bad"][..], "-23:59"),
        (&["blocks", "--session-length", "NaN"][..], "有限正数"),
        (&["blocks", "--token-limit", "0"][..], "正整数"),
        (&["logs", "--limit", "bad"][..], "非负整数"),
    ] {
        let args: Vec<OsString> = std::iter::once("llmusage")
            .chain(args.iter().copied())
            .map(Into::into)
            .collect();
        let root = entrypoint::schema();
        let path = entrypoint::select(&root, &args).path;
        let error = commands::Cli::try_parse_from(args).unwrap_err();
        assert!(
            cli_errors::parser_error(&error, &root, &path, Language::Chinese).contains(expected),
            "{expected}: {error:?}"
        );
    }
}

#[tokio::test]
async fn async_interleaving_nested_sync_scopes_and_unwind_do_not_leak() {
    assert_eq!(i18n::current(), Language::English);
    let english = tokio::spawn(i18n::scope(Language::English, async {
        for _ in 0..50 {
            tokio::task::yield_now().await;
            assert_eq!(i18n::current(), Language::English);
        }
    }));
    let chinese = tokio::spawn(i18n::scope(Language::Chinese, async {
        for _ in 0..50 {
            tokio::task::yield_now().await;
            assert_eq!(i18n::current(), Language::Chinese);
            i18n::with_language(Language::English, || {
                assert_eq!(i18n::current(), Language::English);
                i18n::with_language(Language::Chinese, || {
                    assert_eq!(i18n::current(), Language::Chinese)
                });
                assert_eq!(i18n::current(), Language::English);
            });
            assert_eq!(i18n::current(), Language::Chinese);
        }
        assert!(
            std::panic::catch_unwind(|| i18n::with_language(Language::English, || panic!(
                "restore scope"
            )))
            .is_err()
        );
        assert_eq!(i18n::current(), Language::Chinese);
        assert_eq!(
            tokio::spawn(async { i18n::current() }).await.unwrap(),
            Language::English
        );
    }));
    english.await.unwrap();
    chinese.await.unwrap();
    assert_eq!(i18n::current(), Language::English);
}

#[test]
fn application_error_is_printed_once_with_typed_reason_and_source() -> Result<()> {
    let temp = TempDir::new()?;
    preferences::write(temp.path(), Language::Chinese)?;
    let output = invoke(temp.path(), &["daily"])?;
    assert_eq!(output.status.code(), Some(1));
    let error = stderr(&output);
    assert_eq!(error.matches("错误:").count(), 1, "{error}");
    assert!(
        error.contains("尚未初始化") && error.contains("llmusage init"),
        "{error}"
    );
    let cause = anyhow::Error::new(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "denied by OS",
    ))
    .context(cli_errors::Diagnostic::new("Cannot save", "无法保存"));
    let rendered = cli_errors::application_error(&cause, Language::Chinese);
    assert!(rendered.contains("无法保存") && rendered.contains("denied by OS"));
    Ok(())
}

#[test]
fn sync_failure_finishes_run_log_through_process_adapter() -> Result<()> {
    let fixture = super::ReportCliFixture::new()?;
    fixture.seed_event(super::SeedEvent {
        event_key: "codex:language-failure:1",
        input_tokens: 10,
        total_tokens: 10,
        ..super::SeedEvent::default()
    })?;
    let conn = rusqlite::Connection::open(&fixture.paths.db_path)?;
    conn.execute("INSERT INTO source_file(source, file_path, state, last_seen_at, last_state_change_at) VALUES ('codex', ?1, 'missing', NULL, '2026-06-02T00:00:00Z')", [fixture.home.join("missing.jsonl").to_string_lossy().to_string()])?;
    drop(conn);
    let output = fixture.output_with_env(
        &["sync", "--rebuild", "--source", "codex"],
        &[("LLMUSAGE_LOG", "off"), ("RUST_LOG", "off")],
    )?;
    let runs = llmusage::Store::new(&fixture.paths)?
        .run_log()
        .recent_runs(5)?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(runs[0].status, "failed", "{output:?}: {runs:?}");
    Ok(())
}
