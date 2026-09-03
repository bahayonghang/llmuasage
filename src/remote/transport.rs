use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
    process::{Command, Stdio},
    thread,
};

use crate::{
    error::{LlmusageError, Result},
    store::Host,
};

const DEFAULT_SSH_CONNECT_TIMEOUT_SECS: u64 = 15;
const STDERR_LIMIT: usize = 4096;

/// Split a configured remote command on whitespace. Not passed through a shell.
pub fn split_remote_command(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_string).collect()
}

/// Reject a destination that OpenSSH would parse as an option.
pub fn validate_ssh_target(ssh_target: &str) -> Result<()> {
    if ssh_target.starts_with('-') {
        return Err(LlmusageError::ConfigInvalid {
            detail: format!(
                "ssh_target {ssh_target:?} must not start with '-'; \
                 OpenSSH would treat it as an option"
            ),
        });
    }
    Ok(())
}

/// Local `ssh` argv. `--` precedes destination. Remote sshd still shells the command.
pub fn ssh_args(
    timeout_secs: u64,
    ssh_target: &str,
    remote_argv: &[String],
) -> Result<Vec<String>> {
    validate_ssh_target(ssh_target)?;
    let mut args = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        format!("ConnectTimeout={timeout_secs}"),
        "--".to_string(),
        ssh_target.to_string(),
    ];
    args.extend(remote_argv.iter().cloned());
    Ok(args)
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct RemoteCommandRequest {
    pub ssh_target: String,
    pub argv: Vec<String>,
}

pub trait RemoteCommandRunner: Send + Sync {
    fn run(&self, request: &RemoteCommandRequest) -> Result<CommandOutput>;
}

pub struct SshCommandRunner {
    pub connect_timeout_secs: u64,
}

impl Default for SshCommandRunner {
    fn default() -> Self {
        Self {
            connect_timeout_secs: DEFAULT_SSH_CONNECT_TIMEOUT_SECS,
        }
    }
}

impl RemoteCommandRunner for SshCommandRunner {
    fn run(&self, request: &RemoteCommandRequest) -> Result<CommandOutput> {
        let args = ssh_args(
            self.connect_timeout_secs,
            &request.ssh_target,
            &request.argv,
        )?;
        let output = Command::new("ssh")
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| LlmusageError::ConfigInvalid {
                detail: format!("failed to spawn ssh: {source}"),
            })?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: truncate_stderr(&String::from_utf8_lossy(&output.stderr)),
        })
    }
}

/// Scripted runner for `remote add` tests. Never calls `ssh`.
#[derive(Debug, Clone)]
pub struct ScriptedCommandRunner {
    pub version: CommandOutput,
    pub handshake: CommandOutput,
}

impl RemoteCommandRunner for ScriptedCommandRunner {
    fn run(&self, request: &RemoteCommandRequest) -> Result<CommandOutput> {
        if request.argv.iter().any(|arg| arg == "--version") {
            return Ok(self.version.clone());
        }
        if request.argv.iter().any(|arg| arg == "handshake") {
            return Ok(self.handshake.clone());
        }
        Err(LlmusageError::ConfigInvalid {
            detail: format!("unexpected remote argv: {:?}", request.argv),
        })
    }
}

pub trait ShardSession {
    fn reader(&mut self) -> &mut dyn BufRead;
    fn finish(self: Box<Self>) -> Result<CommandOutput>;
}

pub trait ShardSource: Send + Sync {
    fn open(&self, host: &Host, since: Option<&str>) -> Result<Box<dyn ShardSession>>;
}

pub struct SshShardSource {
    pub connect_timeout_secs: u64,
}

impl Default for SshShardSource {
    fn default() -> Self {
        Self {
            connect_timeout_secs: DEFAULT_SSH_CONNECT_TIMEOUT_SECS,
        }
    }
}

struct SshShardSession {
    child: std::process::Child,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: thread::JoinHandle<String>,
}

impl ShardSession for SshShardSession {
    fn reader(&mut self) -> &mut dyn BufRead {
        &mut self.stdout
    }

    fn finish(mut self: Box<Self>) -> Result<CommandOutput> {
        drop(self.stdout);
        let status = self.child.wait().map_err(LlmusageError::from)?;
        let stderr = self.stderr.join().unwrap_or_default();
        Ok(CommandOutput {
            status: status.code().unwrap_or(1),
            stdout: String::new(),
            stderr,
        })
    }
}

impl ShardSource for SshShardSource {
    fn open(&self, host: &Host, since: Option<&str>) -> Result<Box<dyn ShardSession>> {
        let ssh_target =
            host.ssh_target
                .as_deref()
                .ok_or_else(|| LlmusageError::ConfigInvalid {
                    detail: format!("host {} has no ssh_target", host.host_id),
                })?;
        let mut remote_argv = split_remote_command(&host.command);
        if remote_argv.is_empty() {
            return Err(LlmusageError::ConfigInvalid {
                detail: "remote command is empty".to_string(),
            });
        }
        remote_argv.push("sync".to_string());
        remote_argv.push("--emit-shards".to_string());
        if let Some(since) = since {
            remote_argv.push("--since".to_string());
            remote_argv.push(since.to_string());
        }
        let args = ssh_args(self.connect_timeout_secs, ssh_target, &remote_argv)?;
        let mut child = Command::new("ssh")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| LlmusageError::ConfigInvalid {
                detail: format!("failed to spawn ssh: {source}"),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LlmusageError::ConfigInvalid {
                detail: "ssh stdout was not piped".to_string(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| LlmusageError::ConfigInvalid {
                detail: "ssh stderr was not piped".to_string(),
            })?;
        let stderr = thread::spawn(move || {
            let mut reader = stderr;
            read_truncated(&mut reader, STDERR_LIMIT)
        });
        Ok(Box::new(SshShardSession {
            child,
            stdout: BufReader::new(stdout),
            stderr,
        }))
    }
}

/// In-memory shard stream for tests. Never calls `ssh`.
#[derive(Debug, Clone)]
pub struct MemoryShardSource {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

struct MemoryShardSession {
    reader: BufReader<std::io::Cursor<Vec<u8>>>,
    stderr: String,
    status: i32,
}

impl ShardSession for MemoryShardSession {
    fn reader(&mut self) -> &mut dyn BufRead {
        &mut self.reader
    }

    fn finish(self: Box<Self>) -> Result<CommandOutput> {
        Ok(CommandOutput {
            status: self.status,
            stdout: String::new(),
            stderr: self.stderr,
        })
    }
}

impl ShardSource for MemoryShardSource {
    fn open(&self, _host: &Host, _since: Option<&str>) -> Result<Box<dyn ShardSession>> {
        Ok(Box::new(MemoryShardSession {
            reader: BufReader::new(std::io::Cursor::new(self.stdout.as_bytes().to_vec())),
            stderr: truncate_stderr(&self.stderr),
            status: self.status,
        }))
    }
}

/// Per-host shard streams for tests. Never calls `ssh`.
#[derive(Debug, Clone)]
pub struct ScriptedShardSource {
    /// Configured stream or error for a `host_id`.
    pub hosts: BTreeMap<String, std::result::Result<MemoryShardSource, String>>,
    /// Error used when `hosts` has no entry for the requested host.
    pub default_error: String,
}

impl Default for ScriptedShardSource {
    fn default() -> Self {
        Self {
            hosts: BTreeMap::new(),
            default_error: "remote host is unreachable".to_string(),
        }
    }
}

impl ShardSource for ScriptedShardSource {
    fn open(&self, host: &Host, since: Option<&str>) -> Result<Box<dyn ShardSession>> {
        match self.hosts.get(&host.host_id) {
            Some(Ok(source)) => source.open(host, since),
            Some(Err(detail)) => Err(LlmusageError::ConfigInvalid {
                detail: detail.clone(),
            }),
            None => Err(LlmusageError::ConfigInvalid {
                detail: self.default_error.clone(),
            }),
        }
    }
}

fn truncate_stderr(stderr: &str) -> String {
    let mut bytes = stderr.as_bytes();
    if bytes.len() <= STDERR_LIMIT {
        return stderr.to_string();
    }
    bytes = &bytes[..STDERR_LIMIT];
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    text.push_str("...[truncated]");
    text
}

fn read_truncated(reader: &mut impl Read, limit: usize) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    loop {
        match reader.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() < limit {
                    let take = n.min(limit - buf.len());
                    buf.extend_from_slice(&tmp[..take]);
                }
            }
            Err(_) => break,
        }
    }
    truncate_stderr(&String::from_utf8_lossy(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_remote_command_does_not_use_a_shell() {
        assert_eq!(
            split_remote_command("docker exec c1 llmusage"),
            vec!["docker", "exec", "c1", "llmusage"]
        );
        let args = ssh_args(15, "me@devbox", &split_remote_command("llmusage; rm -rf /"))
            .expect("legal target");
        assert_eq!(args[0], "-o");
        assert_eq!(args[1], "BatchMode=yes");
        let dest = args
            .iter()
            .position(|arg| arg == "me@devbox")
            .expect("destination");
        assert_eq!(args[dest - 1], "--");
        assert!(args.contains(&"me@devbox".to_string()));
        assert!(args.iter().any(|arg| arg.contains("llmusage;")));
        assert!(!args.iter().any(|arg| arg.contains("sh -c")));
    }

    #[test]
    fn ssh_args_inserts_double_dash_before_legal_destinations() {
        for target in ["me@devbox", "devbox", "ssh://user@host"] {
            let args = ssh_args(15, target, &["llmusage".to_string()]).expect(target);
            let dest = args.iter().position(|arg| arg == target).expect(target);
            assert_eq!(args[dest - 1], "--", "{target}");
            assert_eq!(args[dest + 1], "llmusage", "{target}");
        }
        let args = ssh_args(15, "me@devbox", &["llmusage".to_string()]).unwrap();
        assert_eq!(
            args,
            [
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=15",
                "--",
                "me@devbox",
                "llmusage",
            ]
        );
    }

    #[test]
    fn ssh_args_rejects_option_like_targets() {
        for target in ["-o", "-oProxyCommand=bash -c id"] {
            let err = ssh_args(15, target, &[]).expect_err(target);
            assert!(
                matches!(err, LlmusageError::ConfigInvalid { .. }),
                "{target}: {err}"
            );
            let text = err.to_string();
            assert!(text.contains("must not start with '-'"), "{target}: {text}");
        }
    }

    #[test]
    fn ssh_shard_source_rejects_option_like_target_without_spawning() {
        let host = Host {
            host_id: "devbox".to_string(),
            label: "devbox".to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some("-oProxyCommand=true".to_string()),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: None,
            last_error: None,
            import_watermark: None,
        };
        let err = match SshShardSource::default().open(&host, None) {
            Ok(_) => panic!("option-like target should be rejected"),
            Err(err) => err,
        };
        assert!(matches!(err, LlmusageError::ConfigInvalid { .. }), "{err}");
        let text = err.to_string();
        assert!(
            text.contains("must not start with '-'"),
            "spawn failure must not count as rejection: {text}"
        );
    }
}
