use std::io::BufRead;

use serde::{Deserialize, Serialize};

use crate::{
    error::{LlmusageError, Result},
    models::ParseIssues,
    parsers::SourceSyncStats,
    store::{SyncShard, latest_schema_version},
};

/// Wire version for NDJSON shard records. Schema version is diagnostic only.
pub const SHARD_PROTOCOL_VERSION: u32 = 1;

/// One NDJSON record in a shard stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ShardRecord {
    Header {
        shard_protocol: u32,
        llmusage_version: String,
        schema_version: u32,
        emitted_at: String,
    },
    Shard {
        shard: SyncShard,
    },
    Trailer {
        sources: Vec<SourceSyncStats>,
        parse_issues: ParseIssues,
    },
}

/// JSON payload of the hidden `remote handshake` command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub shard_protocol: u32,
    pub schema_version: u32,
    pub llmusage_version: String,
}

impl HandshakeResponse {
    pub fn local() -> Self {
        Self {
            shard_protocol: SHARD_PROTOCOL_VERSION,
            schema_version: latest_schema_version(),
            llmusage_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

pub fn encode_record(record: &ShardRecord) -> Result<String> {
    serde_json::to_string(record).map_err(|source| LlmusageError::Parse {
        context: "shard record",
        source,
    })
}

pub fn protocol_mismatch_error(remote_protocol: u32, remote_schema_version: u32) -> LlmusageError {
    LlmusageError::ConfigInvalid {
        detail: format!(
            "shard protocol mismatch: local={SHARD_PROTOCOL_VERSION} remote={remote_protocol} \
             (remote schema_version={remote_schema_version}); upgrade llmusage so both sides \
             share shard_protocol {SHARD_PROTOCOL_VERSION}"
        ),
    }
}

/// Line-oriented decoder. Non-JSON and unknown `kind` lines are skipped locally.
pub struct ShardDecoder<R> {
    reader: R,
    skipped_lines: u64,
    header_seen: bool,
    trailer_seen: bool,
}

impl<R: BufRead> ShardDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            skipped_lines: 0,
            header_seen: false,
            trailer_seen: false,
        }
    }

    pub fn skipped_lines(&self) -> u64 {
        self.skipped_lines
    }

    pub fn saw_header(&self) -> bool {
        self.header_seen
    }

    pub fn saw_trailer(&self) -> bool {
        self.trailer_seen
    }

    pub fn next_record(&mut self) -> Result<Option<ShardRecord>> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .reader
                .read_line(&mut line)
                .map_err(LlmusageError::from)?;
            if read == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<ShardRecord>(trimmed) {
                Ok(record) => {
                    if !self.header_seen {
                        match &record {
                            ShardRecord::Header {
                                shard_protocol,
                                schema_version,
                                ..
                            } => {
                                if *shard_protocol != SHARD_PROTOCOL_VERSION {
                                    return Err(protocol_mismatch_error(
                                        *shard_protocol,
                                        *schema_version,
                                    ));
                                }
                                self.header_seen = true;
                                return Ok(Some(record));
                            }
                            _ => {
                                return Err(LlmusageError::ConfigInvalid {
                                    detail: "first deserialized shard record must be a header"
                                        .to_string(),
                                });
                            }
                        }
                    }
                    if self.trailer_seen {
                        self.skipped_lines = self.skipped_lines.saturating_add(1);
                        continue;
                    }
                    if matches!(record, ShardRecord::Trailer { .. }) {
                        self.trailer_seen = true;
                    }
                    return Ok(Some(record));
                }
                Err(_) => {
                    self.skipped_lines = self.skipped_lines.saturating_add(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SourceKind;
    use crate::store::RawRecord;
    use std::io::Cursor;

    fn decoder(text: &str) -> ShardDecoder<Cursor<Vec<u8>>> {
        ShardDecoder::new(Cursor::new(text.as_bytes().to_vec()))
    }

    fn header() -> ShardRecord {
        ShardRecord::Header {
            shard_protocol: SHARD_PROTOCOL_VERSION,
            llmusage_version: "1.2.0".to_string(),
            schema_version: 23,
            emitted_at: "2026-08-20T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn skips_leading_non_json_and_requires_header() -> anyhow::Result<()> {
        let mut stream = decoder(&format!(
            "Welcome to the host\n\n{}\n",
            encode_record(&header())?
        ));
        let first = stream.next_record()?.expect("header");
        assert!(matches!(first, ShardRecord::Header { .. }));
        assert_eq!(stream.skipped_lines(), 1);
        assert!(stream.saw_header());
        Ok(())
    }

    #[test]
    fn first_deserialized_non_header_fails_without_counting_motd() -> anyhow::Result<()> {
        let shard = ShardRecord::Shard {
            shard: SyncShard::new(SourceKind::Codex),
        };
        let mut stream = decoder(&format!("banner\n{}\n", encode_record(&shard)?));
        let err = stream.next_record().expect_err("must reject");
        assert!(
            err.to_string()
                .contains("first deserialized shard record must be a header"),
            "{err}"
        );
        assert_eq!(stream.skipped_lines(), 1);
        Ok(())
    }

    #[test]
    fn protocol_mismatch_fails_immediately() -> anyhow::Result<()> {
        let mut stream = decoder(
            r#"{"kind":"header","shard_protocol":99,"llmusage_version":"0.1.0","schema_version":22,"emitted_at":"2026-08-20T00:00:00Z"}
"#,
        );
        let err = stream.next_record().expect_err("mismatch");
        let text = err.to_string();
        assert!(text.contains("local=1"), "{text}");
        assert!(text.contains("remote=99"), "{text}");
        assert!(text.contains("schema_version=22"), "{text}");
        Ok(())
    }

    #[test]
    fn skips_unknown_kind_and_non_json_between_records() -> anyhow::Result<()> {
        let mut stream = decoder(&format!(
            "{}\nnot-json\n{{\"kind\":\"future\"}}\n{}\n",
            encode_record(&header())?,
            encode_record(&ShardRecord::Trailer {
                sources: Vec::new(),
                parse_issues: ParseIssues::default(),
            })?
        ));
        assert!(matches!(
            stream.next_record()?.expect("header"),
            ShardRecord::Header { .. }
        ));
        assert!(matches!(
            stream.next_record()?.expect("trailer"),
            ShardRecord::Trailer { .. }
        ));
        assert_eq!(stream.skipped_lines(), 2);
        Ok(())
    }

    #[test]
    fn serialized_shard_omits_raw_records_and_prompt_text() -> anyhow::Result<()> {
        let secret = "private prompt must never appear in diagnostics";
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard.raw_records.push(RawRecord {
            event_key: "codex:path:1".to_string(),
            raw_json: format!(r#"{{"prompt":"{secret}"}}"#),
        });
        let encoded = encode_record(&ShardRecord::Shard { shard })?;
        assert!(!encoded.contains("raw_records"), "{encoded}");
        assert!(!encoded.contains(secret), "{encoded}");
        Ok(())
    }
}
