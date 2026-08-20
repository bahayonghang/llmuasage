//! SSH shard transport and local import of remote `SyncShard` streams.
//!
//! This module must not depend on `crate::commands`. CLI wiring lives in
//! `src/commands/remote.rs`.

pub mod importer;
pub mod protocol;
pub mod register;
pub mod transport;

pub use importer::{IMPORT_WATERMARK_OVERLAP_HOURS, ImportOutcome, RemoteImporter};
pub use protocol::{
    HandshakeResponse, SHARD_PROTOCOL_VERSION, ShardDecoder, ShardRecord, encode_record,
};
pub use register::{normalize_host_id, register_remote_host, validate_new_host_id};
pub use transport::{
    CommandOutput, MemoryShardSource, RemoteCommandRequest, RemoteCommandRunner,
    ScriptedCommandRunner, ScriptedShardSource, ShardSession, ShardSource, SshCommandRunner,
    SshShardSource, split_remote_command,
};
