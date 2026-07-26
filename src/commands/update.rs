use std::{
    collections::BTreeMap,
    fmt,
    io::{self, BufRead, Write},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::ValueEnum;

const GITHUB_REPOSITORY: &str = "https://github.com/bahayonghang/llmuasage";
const CARGO_PACKAGE: &str = "llmusage";
const DEV_REF: &str = "refs/heads/dev";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum UpdateChannel {
    #[default]
    Main,
    Dev,
}

impl UpdateChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Dev => "dev",
        }
    }
}

impl fmt::Display for UpdateChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedUpdate {
    channel: UpdateChannel,
    tag: Option<String>,
    commit_sha: String,
    mutable_channel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallPlan {
    program: &'static str,
    args: Vec<String>,
    target: ResolvedUpdate,
}

impl InstallPlan {
    fn for_target(target: ResolvedUpdate) -> Self {
        let target_args = if target.mutable_channel {
            vec!["--branch".to_string(), target.channel.as_str().to_string()]
        } else {
            vec!["--rev".to_string(), target.commit_sha.clone()]
        };
        let mut args = vec![
            "install".to_string(),
            "--git".to_string(),
            GITHUB_REPOSITORY.to_string(),
            CARGO_PACKAGE.to_string(),
        ];
        args.extend(target_args);
        args.extend(["--locked".to_string(), "--force".to_string()]);
        Self {
            program: "cargo",
            args,
            target,
        }
    }

    fn command_line(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallOutcome {
    Success,
    Failed(Option<i32>),
}

trait RefProvider {
    fn fetch_refs(&mut self, channel: UpdateChannel) -> Result<String>;
}

struct GitRefProvider;

impl RefProvider for GitRefProvider {
    fn fetch_refs(&mut self, channel: UpdateChannel) -> Result<String> {
        let mut command = Command::new("git");
        command.arg("ls-remote");
        match channel {
            UpdateChannel::Main => {
                command.args(["--tags", GITHUB_REPOSITORY, "refs/tags/*"]);
            }
            UpdateChannel::Dev => {
                command.args(["--heads", GITHUB_REPOSITORY, DEV_REF]);
            }
        }
        let output = command
            .output()
            .with_context(|| format!("failed to resolve update refs for `{channel}`"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!(
                "failed to resolve update refs for `{channel}`{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            );
        }
        String::from_utf8(output.stdout).context("git ls-remote returned non-UTF-8 output")
    }
}

pub fn run(channel: UpdateChannel, check_only: bool) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_with(
        channel,
        check_only,
        &mut stdin.lock(),
        &mut stdout.lock(),
        &mut GitRefProvider,
        execute_install,
    )
}

fn run_with<R, W, P, F>(
    channel: UpdateChannel,
    check_only: bool,
    reader: &mut R,
    writer: &mut W,
    provider: &mut P,
    mut executor: F,
) -> Result<()>
where
    R: BufRead,
    W: Write,
    P: RefProvider,
    F: FnMut(&InstallPlan) -> Result<InstallOutcome>,
{
    let target = resolve_update(channel, provider)?;
    let plan = InstallPlan::for_target(target.clone());
    print_header(writer, &plan)?;

    if check_only {
        writeln!(writer, "Check only: no update was performed.")?;
        return Ok(());
    }

    if !confirm_update(reader, writer)? {
        writeln!(writer, "Update cancelled.")?;
        return Ok(());
    }

    let verified = resolve_update(channel, provider)?;
    ensure!(
        verified == target,
        "resolved update target changed after confirmation; rerun `llmusage update {channel}` to review the new target"
    );

    writeln!(writer, "Starting update...")?;
    writer.flush()?;
    let outcome = executor(&plan).map_err(|error| {
        anyhow!(
            "failed to start Cargo update from `{channel}`: {error:#}\nManual command: {}",
            plan.command_line()
        )
    })?;

    match outcome {
        InstallOutcome::Success => {
            writeln!(writer, "Update completed successfully.")?;
            writeln!(
                writer,
                "Run `llmusage --version` to verify the installed version."
            )?;
            Ok(())
        }
        InstallOutcome::Failed(exit_code) => {
            let detail = exit_code.map_or_else(
                || "terminated without an exit code".to_string(),
                |code| format!("exit code {code}"),
            );
            bail!(
                "Cargo update from `{channel}` failed with {detail}.\nManual command: {}",
                plan.command_line()
            )
        }
    }
}

fn resolve_update(
    channel: UpdateChannel,
    provider: &mut impl RefProvider,
) -> Result<ResolvedUpdate> {
    let refs = provider.fetch_refs(channel)?;
    match channel {
        UpdateChannel::Main => resolve_stable_refs(&refs),
        UpdateChannel::Dev => resolve_dev_ref(&refs),
    }
}

#[derive(Default)]
struct TagTarget {
    direct: Option<String>,
    peeled: Option<String>,
}

fn resolve_stable_refs(raw: &str) -> Result<ResolvedUpdate> {
    let mut tags: BTreeMap<(u64, u64, u64), (String, TagTarget)> = BTreeMap::new();
    for (sha, reference) in parse_ref_lines(raw)? {
        let Some(rest) = reference.strip_prefix("refs/tags/") else {
            continue;
        };
        let (tag, peeled) = rest
            .strip_suffix("^{}")
            .map_or((rest, false), |tag| (tag, true));
        let Some(version) = stable_version(tag) else {
            continue;
        };
        let entry = tags
            .entry(version)
            .or_insert_with(|| (tag.to_string(), TagTarget::default()));
        ensure!(
            entry.0 == tag,
            "ambiguous stable release tags for {version:?}"
        );
        let slot = if peeled {
            &mut entry.1.peeled
        } else {
            &mut entry.1.direct
        };
        if let Some(existing) = slot.as_ref() {
            ensure!(
                existing == &sha,
                "release tag `{tag}` resolved inconsistently"
            );
        } else {
            *slot = Some(sha);
        }
    }

    let (_, (tag, target)) = tags
        .into_iter()
        .next_back()
        .context("no stable semantic-version release tag was found")?;
    ensure!(
        target.direct.is_some(),
        "release tag `{tag}` has a peeled commit without a tag ref"
    );
    let commit_sha = target.peeled.or(target.direct).expect("direct tag checked");
    ensure!(
        is_full_commit_sha(&commit_sha),
        "release tag `{tag}` resolved to an invalid commit SHA"
    );
    Ok(ResolvedUpdate {
        channel: UpdateChannel::Main,
        tag: Some(tag),
        commit_sha,
        mutable_channel: false,
    })
}

fn resolve_dev_ref(raw: &str) -> Result<ResolvedUpdate> {
    let mut commit = None;
    for (sha, reference) in parse_ref_lines(raw)? {
        if reference != DEV_REF {
            continue;
        }
        ensure!(
            is_full_commit_sha(&sha),
            "dev branch resolved to an invalid commit SHA"
        );
        if let Some(existing) = commit.as_ref() {
            ensure!(existing == &sha, "dev branch resolved inconsistently");
        } else {
            commit = Some(sha);
        }
    }
    Ok(ResolvedUpdate {
        channel: UpdateChannel::Dev,
        tag: None,
        commit_sha: commit.context("official dev branch was not found")?,
        mutable_channel: true,
    })
}

fn parse_ref_lines(raw: &str) -> Result<Vec<(String, String)>> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split_whitespace();
            let sha = fields.next().context("missing SHA in git ref output")?;
            let reference = fields
                .next()
                .context("missing ref name in git ref output")?;
            ensure!(
                fields.next().is_none(),
                "unexpected fields in git ref output"
            );
            ensure!(is_full_commit_sha(sha), "invalid SHA in git ref output");
            Ok((sha.to_ascii_lowercase(), reference.to_string()))
        })
        .collect()
}

fn stable_version(tag: &str) -> Option<(u64, u64, u64)> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    if version.contains('-') || version.contains('+') {
        return None;
    }
    let parse_component = |component: &str| {
        if component.len() > 1 && component.starts_with('0') {
            None
        } else {
            component.parse::<u64>().ok()
        }
    };
    let mut parts = version.split('.');
    let parsed = (
        parse_component(parts.next()?)?,
        parse_component(parts.next()?)?,
        parse_component(parts.next()?)?,
    );
    parts.next().is_none().then_some(parsed)
}

fn is_full_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn print_header(writer: &mut impl Write, plan: &InstallPlan) -> Result<()> {
    writeln!(writer, "llmusage update")?;
    writeln!(writer, "Current version: {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(writer, "Repository: {GITHUB_REPOSITORY}")?;
    writeln!(writer, "Channel: {}", plan.target.channel)?;
    if let Some(tag) = &plan.target.tag {
        writeln!(writer, "Release tag: {tag}")?;
    }
    writeln!(writer, "Resolved commit: {}", plan.target.commit_sha)?;
    if plan.target.mutable_channel {
        writeln!(writer)?;
        writeln!(
            writer,
            "WARNING: dev follows the mutable 'dev' branch and is not a verified stable release."
        )?;
        writeln!(
            writer,
            "The resolved commit is shown for review, but the branch can change in the future."
        )?;
    }
    writeln!(writer)?;
    writeln!(writer, "Command: {}", plan.command_line())?;
    writeln!(writer)?;
    Ok(())
}

fn confirm_update(reader: &mut impl BufRead, writer: &mut impl Write) -> Result<bool> {
    loop {
        write!(writer, "Continue with the update? [Y/n]: ")?;
        writer.flush()?;

        let mut input = String::new();
        let bytes_read = reader.read_line(&mut input)?;
        if bytes_read == 0 {
            bail!("confirmation input reached EOF; update was not started");
        }

        match input.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => writeln!(writer, "Please answer `y` or `n`.")?,
        }
    }
}

fn execute_install(plan: &InstallPlan) -> Result<InstallOutcome> {
    let status = Command::new(plan.program)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if status.success() {
        Ok(InstallOutcome::Success)
    } else {
        Ok(InstallOutcome::Failed(status.code()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        collections::VecDeque,
        io::{self, BufReader, Cursor, Read},
    };

    use anyhow::anyhow;

    use super::*;

    const SHA_A: &str = "1111111111111111111111111111111111111111";
    const SHA_B: &str = "2222222222222222222222222222222222222222";
    const SHA_256: &str = "3333333333333333333333333333333333333333333333333333333333333333";
    const TAG_OBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct FakeRefProvider {
        responses: VecDeque<Result<String, &'static str>>,
        calls: usize,
    }

    impl FakeRefProvider {
        fn new(responses: impl IntoIterator<Item = String>) -> Self {
            Self {
                responses: responses.into_iter().map(Ok).collect(),
                calls: 0,
            }
        }

        fn failing(message: &'static str) -> Self {
            Self {
                responses: VecDeque::from([Err(message)]),
                calls: 0,
            }
        }
    }

    impl RefProvider for FakeRefProvider {
        fn fetch_refs(&mut self, _channel: UpdateChannel) -> Result<String> {
            self.calls += 1;
            self.responses
                .pop_front()
                .context("unexpected ref provider call")?
                .map_err(anyhow::Error::msg)
        }
    }

    fn stable_refs(tag: &str, commit: &str) -> String {
        format!("{TAG_OBJECT}\trefs/tags/{tag}\n{commit}\trefs/tags/{tag}^{{}}\n")
    }

    fn dev_refs(commit: &str) -> String {
        format!("{commit}\t{DEV_REF}\n")
    }

    fn repeated_refs(raw: String) -> FakeRefProvider {
        FakeRefProvider::new([raw.clone(), raw])
    }

    #[test]
    fn stable_plan_uses_resolved_release_commit_and_dev_remains_explicitly_mutable() -> Result<()> {
        let stable = resolve_stable_refs(&format!(
            "{}{}",
            stable_refs("v1.0.2", SHA_A),
            stable_refs("v1.1.0", SHA_B)
        ))?;
        let main = InstallPlan::for_target(stable.clone());
        assert_eq!(stable.tag.as_deref(), Some("v1.1.0"));
        assert_eq!(stable.commit_sha, SHA_B);
        assert_eq!(main.args[4..6], ["--rev", SHA_B]);
        assert!(!main.args.iter().any(|arg| arg == "main"));

        let dev = resolve_dev_ref(&dev_refs(SHA_A))?;
        let dev_plan = InstallPlan::for_target(dev.clone());
        assert!(dev.mutable_channel);
        assert_eq!(dev_plan.args[4..6], ["--branch", "dev"]);
        Ok(())
    }

    #[test]
    fn stable_resolver_accepts_lightweight_tags_and_sha256_object_ids() -> Result<()> {
        let lightweight = resolve_stable_refs(&format!(
            "{SHA_A}\trefs/tags/v1.2.3\n{SHA_256}\trefs/tags/v1.3.0\n"
        ))?;

        assert_eq!(lightweight.tag.as_deref(), Some("v1.3.0"));
        assert_eq!(lightweight.commit_sha, SHA_256);
        Ok(())
    }

    #[test]
    fn stable_resolver_excludes_prerelease_build_and_noncanonical_versions() -> Result<()> {
        let resolved = resolve_stable_refs(&format!(
            "{SHA_A}\trefs/tags/v1.2.3\n\
             {SHA_B}\trefs/tags/v9.0.0-beta.1\n\
             {SHA_B}\trefs/tags/v8.0.0+build.1\n\
             {SHA_B}\trefs/tags/v07.0.0\n\
             {SHA_B}\trefs/tags/v18446744073709551616.0.0\n"
        ))?;

        assert_eq!(resolved.tag.as_deref(), Some("v1.2.3"));
        assert_eq!(resolved.commit_sha, SHA_A);
        for tag in ["v01.0.0", "v1.00.0", "v1.0.00", "v18446744073709551616.0.0"] {
            let raw = format!("{SHA_A}\trefs/tags/{tag}\n");
            assert!(resolve_stable_refs(&raw).is_err(), "tag={tag}");
        }
        Ok(())
    }

    #[test]
    fn resolver_rejects_invalid_ambiguous_and_missing_release_targets() {
        for raw in [
            "short\trefs/tags/v1.0.0\n".to_string(),
            format!("{SHA_A}\trefs/tags/v1.0.0\n{SHA_B}\trefs/tags/v1.0.0\n"),
            format!("{SHA_A}\trefs/tags/v1.0.0^{{}}\n"),
            format!("{SHA_A}\trefs/tags/v2.0.0-beta.1\n"),
        ] {
            assert!(resolve_stable_refs(&raw).is_err(), "raw={raw:?}");
        }
    }

    #[test]
    fn dev_resolver_rejects_invalid_conflicting_and_missing_refs() {
        for raw in [
            String::new(),
            format!("short\t{DEV_REF}\n"),
            format!("{SHA_A}\t{DEV_REF}\n{SHA_B}\t{DEV_REF}\n"),
        ] {
            assert!(resolve_dev_ref(&raw).is_err(), "raw={raw:?}");
        }
    }

    #[test]
    fn confirmation_accepts_default_yes_and_no() -> Result<()> {
        let mut output = Vec::new();
        assert!(confirm_update(&mut Cursor::new("\n"), &mut output)?);
        assert!(confirm_update(&mut Cursor::new("YES\n"), &mut output)?);
        assert!(!confirm_update(&mut Cursor::new("no\n"), &mut output)?);
        Ok(())
    }

    #[test]
    fn confirmation_reprompts_invalid_input_and_refuses_eof() -> Result<()> {
        let mut output = Vec::new();
        assert!(confirm_update(&mut Cursor::new("maybe\ny\n"), &mut output)?);
        assert!(String::from_utf8(output)?.contains("Please answer"));

        let error = confirm_update(&mut Cursor::new(Vec::<u8>::new()), &mut Vec::new())
            .expect_err("EOF must not confirm an update");
        assert!(error.to_string().contains("EOF"));
        Ok(())
    }

    #[test]
    fn confirmation_read_failure_never_calls_executor() {
        struct FailingReader;

        impl Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("input unavailable"))
            }
        }

        let called = Cell::new(false);
        let mut provider = FakeRefProvider::new([stable_refs("v1.0.2", SHA_A)]);
        let error = run_with(
            UpdateChannel::Main,
            false,
            &mut BufReader::new(FailingReader),
            &mut Vec::new(),
            &mut provider,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )
        .expect_err("confirmation I/O failure must stop the update");

        assert!(!called.get());
        assert_eq!(provider.calls, 1);
        assert!(error.to_string().contains("input unavailable"));
    }

    #[test]
    fn check_only_resolves_once_and_never_calls_executor() -> Result<()> {
        let called = Cell::new(false);
        let mut output = Vec::new();
        let mut provider = FakeRefProvider::new([stable_refs("v1.0.2", SHA_A)]);
        run_with(
            UpdateChannel::Main,
            true,
            &mut Cursor::new(Vec::<u8>::new()),
            &mut output,
            &mut provider,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )?;

        assert!(!called.get());
        assert_eq!(provider.calls, 1);
        let output = String::from_utf8(output)?;
        assert!(output.contains("Release tag: v1.0.2"));
        assert!(output.contains(&format!("Resolved commit: {SHA_A}")));
        assert!(output.contains(&format!("--rev {SHA_A} --locked --force")));
        assert!(output.contains("no update was performed"));
        Ok(())
    }

    #[test]
    fn cancellation_never_revalidates_or_calls_executor() -> Result<()> {
        let called = Cell::new(false);
        let mut output = Vec::new();
        let mut provider = FakeRefProvider::new([dev_refs(SHA_A)]);
        run_with(
            UpdateChannel::Dev,
            false,
            &mut Cursor::new("n\n"),
            &mut output,
            &mut provider,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )?;

        assert!(!called.get());
        assert_eq!(provider.calls, 1);
        let output = String::from_utf8(output)?;
        assert!(output.contains("Update cancelled"));
        assert!(output.contains("mutable 'dev' branch"));
        assert!(output.contains(SHA_A));
        Ok(())
    }

    #[test]
    fn changed_target_after_confirmation_fails_before_executor() {
        let called = Cell::new(false);
        let mut provider =
            FakeRefProvider::new([stable_refs("v1.0.2", SHA_A), stable_refs("v1.0.2", SHA_B)]);
        let error = run_with(
            UpdateChannel::Main,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            &mut provider,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )
        .expect_err("moved tag must stop installation");
        assert!(!called.get());
        assert!(error.to_string().contains("changed after confirmation"));
    }

    #[test]
    fn provider_failure_is_fail_closed() {
        let called = Cell::new(false);
        let error = run_with(
            UpdateChannel::Main,
            true,
            &mut Cursor::new(Vec::<u8>::new()),
            &mut Vec::new(),
            &mut FakeRefProvider::failing("network unavailable"),
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )
        .expect_err("resolution failure must fail closed");
        assert!(!called.get());
        assert!(error.to_string().contains("network unavailable"));
    }

    #[test]
    fn provider_failure_after_confirmation_never_calls_executor() {
        let called = Cell::new(false);
        let mut provider = FakeRefProvider {
            responses: VecDeque::from([
                Ok(stable_refs("v1.0.2", SHA_A)),
                Err("network unavailable after confirmation"),
            ]),
            calls: 0,
        };
        let error = run_with(
            UpdateChannel::Main,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            &mut provider,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )
        .expect_err("post-confirmation resolution failure must fail closed");

        assert!(!called.get());
        assert_eq!(provider.calls, 2);
        assert!(
            error
                .to_string()
                .contains("network unavailable after confirmation")
        );
    }

    #[test]
    fn executor_success_and_failures_are_propagated() -> Result<()> {
        let called = Cell::new(false);
        let refs = dev_refs(SHA_A);
        run_with(
            UpdateChannel::Dev,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            &mut repeated_refs(refs),
            |plan| {
                called.set(true);
                assert_eq!(plan.target.commit_sha, SHA_A);
                assert_eq!(plan.args[5], "dev");
                Ok(InstallOutcome::Success)
            },
        )?;
        assert!(called.get());

        let refs = dev_refs(SHA_A);
        let exit_error = run_with(
            UpdateChannel::Dev,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            &mut repeated_refs(refs),
            |_| Ok(InstallOutcome::Failed(Some(101))),
        )
        .expect_err("non-zero Cargo exit must fail");
        let exit_error = exit_error.to_string();
        assert!(exit_error.contains("exit code 101"));
        assert!(exit_error.contains("--branch dev --locked --force"));

        let refs = stable_refs("v1.0.2", SHA_A);
        let start_error = run_with(
            UpdateChannel::Main,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            &mut repeated_refs(refs),
            |_| Err(anyhow!("cargo is missing")),
        )
        .expect_err("Cargo startup failure must fail");
        let start_error = start_error.to_string();
        assert!(start_error.contains("cargo is missing"));
        assert!(start_error.contains(&format!("--rev {SHA_A} --locked --force")));
        Ok(())
    }
}
