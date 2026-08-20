use crate::{
    error::{LlmusageError, Result},
    registry,
    store::{Host, Store},
    util::now_utc,
};

use super::protocol::{HandshakeResponse, SHARD_PROTOCOL_VERSION, protocol_mismatch_error};
use super::transport::{RemoteCommandRequest, RemoteCommandRunner, split_remote_command};

/// Lowercase the label and replace characters outside `[a-z0-9_-]` with `-`.
pub fn normalize_host_id(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            let lower = ch.to_ascii_lowercase();
            if lower.is_ascii_alphanumeric() || lower == '_' || lower == '-' {
                lower
            } else {
                '-'
            }
        })
        .collect()
}

pub fn validate_new_host_id(store: &Store, host_id: &str) -> Result<()> {
    if host_id.is_empty() {
        return Err(LlmusageError::ConfigInvalid {
            detail: "host label normalizes to an empty host_id".to_string(),
        });
    }
    for descriptor in registry::registered_source_descriptors() {
        if descriptor.kind.as_str() == host_id {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "host_id '{host_id}' collides with source name '{}'; choose a different label",
                    descriptor.kind.as_str()
                ),
            });
        }
    }
    for host in store.hosts().list()? {
        if host.host_id == host_id {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "host_id '{host_id}' is already registered; choose a different label"
                ),
            });
        }
    }
    Ok(())
}

pub fn parse_handshake_stdout(stdout: &str) -> Result<HandshakeResponse> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<HandshakeResponse>(trimmed) {
            return Ok(value);
        }
    }
    serde_json::from_str(stdout.trim()).map_err(|source| LlmusageError::Parse {
        context: "remote handshake",
        source,
    })
}

pub fn register_remote_host(
    store: &Store,
    label: &str,
    ssh_target: &str,
    command: &str,
    runner: &dyn RemoteCommandRunner,
) -> Result<Host> {
    let host_id = normalize_host_id(label);
    validate_new_host_id(store, &host_id)?;
    let command = if command.trim().is_empty() {
        "llmusage"
    } else {
        command
    };
    let command_argv = split_remote_command(command);
    if command_argv.is_empty() {
        return Err(LlmusageError::ConfigInvalid {
            detail: "remote command is empty".to_string(),
        });
    }

    let mut version_argv = command_argv.clone();
    version_argv.push("--version".to_string());
    let version = runner.run(&RemoteCommandRequest {
        ssh_target: ssh_target.to_string(),
        argv: version_argv,
    })?;
    if version.status != 0 {
        return Err(LlmusageError::ConfigInvalid {
            detail: format!(
                "remote llmusage was not found or --version failed (exit {}): {}",
                version.status,
                version.stderr.trim()
            ),
        });
    }

    let mut handshake_argv = command_argv;
    handshake_argv.push("remote".to_string());
    handshake_argv.push("handshake".to_string());
    let handshake_out = runner.run(&RemoteCommandRequest {
        ssh_target: ssh_target.to_string(),
        argv: handshake_argv,
    })?;
    if handshake_out.status != 0 {
        return Err(LlmusageError::ConfigInvalid {
            detail: format!(
                "remote handshake failed (exit {}): {}",
                handshake_out.status,
                handshake_out.stderr.trim()
            ),
        });
    }
    let handshake = parse_handshake_stdout(&handshake_out.stdout)?;
    if handshake.shard_protocol != SHARD_PROTOCOL_VERSION {
        return Err(protocol_mismatch_error(
            handshake.shard_protocol,
            handshake.schema_version,
        ));
    }

    let host = Host {
        host_id,
        label: label.to_string(),
        transport: "ssh".to_string(),
        ssh_target: Some(ssh_target.to_string()),
        command: command.to_string(),
        added_at: now_utc(),
        last_contacted_at: None,
        last_error: None,
        import_watermark: None,
    };
    store.hosts().upsert(&host)?;
    Ok(host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::remote::protocol::HandshakeResponse;
    use crate::remote::transport::{CommandOutput, ScriptedCommandRunner};
    use tempfile::TempDir;

    fn fenced_store() -> anyhow::Result<(tempfile::TempDir, Store, crate::store::WorkerLock)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        let lock = store.acquire_worker_lock_with(
            std::time::Duration::from_secs(5),
            crate::store::HolderKind::Cli,
        )?;
        let fenced = lock.fenced_store();
        fenced.bootstrap()?;
        Ok((temp, fenced, lock))
    }

    fn ok_runner(handshake: HandshakeResponse) -> ScriptedCommandRunner {
        ScriptedCommandRunner {
            version: CommandOutput {
                status: 0,
                stdout: "llmusage 1.2.0\n".to_string(),
                stderr: String::new(),
            },
            handshake: CommandOutput {
                status: 0,
                stdout: serde_json::to_string(&handshake).expect("handshake json"),
                stderr: String::new(),
            },
        }
    }

    #[test]
    fn normalize_host_id_lowercases_and_replaces() {
        assert_eq!(normalize_host_id("Dev Box"), "dev-box");
        assert_eq!(normalize_host_id("gpu_1"), "gpu_1");
    }

    #[test]
    fn add_rejects_missing_remote_binary() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        let runner = ScriptedCommandRunner {
            version: CommandOutput {
                status: 127,
                stdout: String::new(),
                stderr: "bash: llmusage: command not found".to_string(),
            },
            handshake: CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
        };
        let err = register_remote_host(&store, "devbox", "me@devbox", "llmusage", &runner)
            .expect_err("missing binary");
        assert!(err.to_string().contains("not found"), "{err}");
        assert_eq!(store.hosts().list()?.len(), 1);
        Ok(())
    }

    #[test]
    fn add_rejects_protocol_mismatch_and_allows_schema_skew() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        let mismatch = ok_runner(HandshakeResponse {
            shard_protocol: 99,
            schema_version: 22,
            llmusage_version: "0.9.0".to_string(),
        });
        let err = register_remote_host(&store, "devbox", "me@devbox", "llmusage", &mismatch)
            .expect_err("protocol");
        let text = err.to_string();
        assert!(text.contains("local=1"), "{text}");
        assert!(text.contains("remote=99"), "{text}");
        assert_eq!(store.hosts().list()?.len(), 1);

        let schema_skew = ok_runner(HandshakeResponse {
            shard_protocol: SHARD_PROTOCOL_VERSION,
            schema_version: latest_schema_for_test() + 7,
            llmusage_version: "9.9.9".to_string(),
        });
        register_remote_host(&store, "devbox", "me@devbox", "llmusage", &schema_skew)?;
        assert!(store.hosts().get_by_label("devbox")?.is_some());
        Ok(())
    }

    fn latest_schema_for_test() -> u32 {
        crate::store::latest_schema_version()
    }

    #[test]
    fn add_rejects_host_id_collision_with_source_or_existing_host() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        let runner = ok_runner(HandshakeResponse::local());
        let err = register_remote_host(&store, "codex", "me@devbox", "llmusage", &runner)
            .expect_err("source name");
        assert!(err.to_string().contains("source name"), "{err}");

        let err = register_remote_host(&store, "local", "me@devbox", "llmusage", &runner)
            .expect_err("local");
        assert!(err.to_string().contains("already registered"), "{err}");
        assert_eq!(store.hosts().list()?.len(), 1);
        Ok(())
    }
}
