//! Native Antigravity metadata decoding. Field names are confirmed against
//! the installed CLI 1.2.5 and IDE 2.5.5 protobuf descriptors.
use crate::{
    models::{ParseIssueKind, ParseIssues, SessionInfo, SourceKind, UsageEvent, UsageTokens},
    project::ProjectResolver,
    util::{bucket_start_from_rfc3339, hash_string, normalize_model},
};
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Usage {
    model_id: Option<u64>,
    input: u64,
    output_total: u64,
    cache_write: u64,
    cache_read: u64,
    thinking: u64,
    visible: u64,
    identities: BTreeSet<String>,
    completeness: usize,
}

#[derive(Clone, Debug)]
pub(super) struct Observation {
    pub source: SourceKind,
    product_proven: bool,
    pub path_hash: String,
    location: String,
    usage: Usage,
    model: Option<String>,
    label: Option<String>,
    time: Option<(u8, i64)>,
    workspace: Option<PathBuf>,
}

pub(super) struct DecodedFile {
    pub source: SourceKind,
    pub observations: Vec<Observation>,
    pub root_attribution: bool,
}

pub(super) fn read_file(
    path: &Path,
    source: SourceKind,
    cancel: &CancellationToken,
) -> Result<DecodedFile> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(std::time::Duration::from_millis(250))?;
    connection.execute_batch("PRAGMA query_only=ON; BEGIN")?;
    let product = product_source(&connection)?;
    let source = product.unwrap_or(source);
    let path_hash = hash_string(&path.to_string_lossy());
    let (session_time, workspace) = trajectory(&connection)?;
    if !table_exists(&connection, "gen_metadata")? && !table_exists(&connection, "steps")? {
        bail!("unrecognized Antigravity SQLite schema; history preserved");
    }
    let mut observations = Vec::new();
    if table_exists(&connection, "gen_metadata")? {
        let mut statement =
            connection.prepare("SELECT idx, data FROM gen_metadata ORDER BY idx")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            if cancel.is_cancelled() {
                bail!("Antigravity metadata read cancelled");
            }
            let idx: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let fields = decode_fields(&blob)?;
            check_types(&fields, &[], &[1])?;
            let Some(chat) = bytes(&fields, 1) else {
                continue;
            };
            let chat = decode_fields(chat)?;
            let direct_time = bytes(&chat, 9)
                .map(decode_fields)
                .transpose()?
                .and_then(|generation| bytes(&generation, 4).map(Vec::from))
                .map(|timestamp| parse_timestamp(&timestamp))
                .transpose()?
                .flatten();
            let time = direct_time
                .map(|time| (3, time))
                .or(session_time.map(|time| (0, time)));
            let model = text(&chat, 22).or_else(|| text(&chat, 19));
            let label = text(&chat, 21);
            push_usage(
                &mut observations,
                &chat,
                4,
                17,
                source,
                &path_hash,
                &format!("gen:{idx}"),
                model,
                label,
                time,
                workspace.clone(),
            )?;
        }
    }
    if table_exists(&connection, "steps")? {
        // Only metadata is read. Payloads, error details and render text are not selected.
        let mut statement = connection
            .prepare("SELECT idx, metadata FROM steps WHERE metadata IS NOT NULL ORDER BY idx")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            if cancel.is_cancelled() {
                bail!("Antigravity metadata read cancelled");
            }
            let idx: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let fields = decode_fields(&blob)?;
            let time = match bytes(&fields, 8) {
                Some(value) => parse_timestamp(value)?.map(|time| (2, time)),
                None => None,
            }
            .or(match bytes(&fields, 1) {
                Some(value) => parse_timestamp(value)?.map(|time| (1, time)),
                None => None,
            })
            .or(session_time.map(|time| (0, time)));
            let model_info = bytes(&fields, 24)
                .map(decode_fields)
                .transpose()?
                .unwrap_or_default();
            let model = text(&model_info, 12).or_else(|| text(&model_info, 8));
            push_usage(
                &mut observations,
                &fields,
                9,
                28,
                source,
                &path_hash,
                &format!("step:{idx}"),
                model,
                None,
                time,
                workspace.clone(),
            )?;
        }
    }
    // Read transaction keeps gen_metadata, steps and trajectory at one SQLite snapshot.
    connection.execute_batch("ROLLBACK")?;
    let mut label_models: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut id_models: HashMap<u64, BTreeSet<String>> = HashMap::new();
    let mut models = BTreeSet::new();
    for observation in &observations {
        if let Some(model) = &observation.model {
            models.insert(model.clone());
            if let Some(id) = observation.usage.model_id {
                id_models.entry(id).or_default().insert(model.clone());
            }
            if let Some(label) = &observation.label {
                label_models
                    .entry(label.clone())
                    .or_default()
                    .insert(model.clone());
            }
        }
    }
    for observation in &mut observations {
        observation.product_proven = product.is_some();
        if observation.model.is_none() {
            observation.model = observation
                .label
                .as_ref()
                .and_then(|label| label_models.get(label))
                .filter(|models| models.len() == 1)
                .and_then(|models| models.first().cloned())
                .or_else(|| {
                    observation
                        .usage
                        .model_id
                        .and_then(|id| id_models.get(&id))
                        .filter(|models| models.len() == 1)
                        .and_then(|models| models.first().cloned())
                })
                .or_else(|| {
                    (observation.usage.model_id.is_none() && models.len() == 1)
                        .then(|| models.first().cloned())
                        .flatten()
                });
        }
    }
    Ok(DecodedFile {
        source,
        observations,
        root_attribution: product.is_none(),
    })
}

#[allow(clippy::too_many_arguments)]
fn push_usage(
    output: &mut Vec<Observation>,
    fields: &Fields,
    primary: u32,
    retries: u32,
    source: SourceKind,
    path_hash: &str,
    location: &str,
    model: Option<String>,
    label: Option<String>,
    time: Option<(u8, i64)>,
    workspace: Option<PathBuf>,
) -> Result<()> {
    check_types(fields, &[], &[primary, retries])?;
    let mut usages = Vec::new();
    for (idx, blob) in all_bytes(fields, retries).enumerate() {
        let retry = decode_fields(blob)?;
        check_types(&retry, &[], &[2])?;
        if let Some(blob) = bytes(&retry, 2) {
            usages.push((format!("retry:{idx}"), decode_usage(blob)?));
        }
    }
    // Native multi-attempt records prove primary usage is the sum of retries,
    // often sharing retry zero's response id. It must never be added again.
    usages.retain(|(_, usage)| {
        usage.input != 0
            || usage.cache_write != 0
            || usage.cache_read != 0
            || usage.output_total != 0
            || usage.thinking != 0
            || usage.visible != 0
    });
    if usages.is_empty()
        && let Some(blob) = bytes(fields, primary)
    {
        usages.push(("primary".to_owned(), decode_usage(blob)?));
    }
    for (kind, usage) in usages {
        if usage.input == 0
            && usage.cache_write == 0
            && usage.cache_read == 0
            && usage.output_total == 0
            && usage.thinking == 0
            && usage.visible == 0
        {
            continue;
        }
        output.push(Observation {
            source,
            product_proven: false,
            path_hash: path_hash.to_owned(),
            location: format!("{location}:{kind}"),
            usage,
            model: model.clone(),
            label: label.clone(),
            time,
            workspace: workspace.clone(),
        });
    }
    Ok(())
}

fn decode_usage(blob: &[u8]) -> Result<Usage> {
    let fields = decode_fields(blob)?;
    check_types(&fields, &[1, 2, 3, 4, 5, 6, 9, 10], &[7, 11, 12])?;
    let mut identities = BTreeSet::new();
    for (field, kind) in [(11, "r"), (12, "p"), (7, "m")] {
        if let Some(value) = bytes(&fields, field) {
            std::str::from_utf8(value)?;
        }
        if let Some(value) = text(&fields, field) {
            identities.insert(format!("{kind}:{}", hash_string(&value)));
        }
    }
    Ok(Usage {
        model_id: number(&fields, 1).filter(|id| *id != 0),
        input: number(&fields, 2).unwrap_or(0),
        output_total: number(&fields, 3).unwrap_or(0),
        cache_write: number(&fields, 4).unwrap_or(0),
        cache_read: number(&fields, 5).unwrap_or(0),
        thinking: number(&fields, 9).unwrap_or(0),
        visible: number(&fields, 10).unwrap_or(0),
        identities,
        completeness: [2, 3, 4, 5, 9, 10]
            .into_iter()
            .filter(|field| number(&fields, *field).is_some())
            .count(),
    })
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}

fn product_source(connection: &Connection) -> Result<Option<SourceKind>> {
    if !table_exists(connection, "trajectory_meta")? {
        return Ok(None);
    }
    let mut statement = connection.prepare("SELECT DISTINCT source FROM trajectory_meta")?;
    let values = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match values.as_slice() {
        [] => Ok(None),
        [17] => Ok(Some(SourceKind::Antigravity)),
        [1] => Ok(Some(SourceKind::AntigravityIde)),
        _ => bail!("unsupported or conflicting Antigravity trajectory product metadata"),
    }
}

fn trajectory(connection: &Connection) -> Result<(Option<i64>, Option<PathBuf>)> {
    if !table_exists(connection, "trajectory_metadata_blob")? {
        return Ok((None, None));
    }
    let mut statement = connection.prepare("SELECT data FROM trajectory_metadata_blob LIMIT 1")?;
    let mut rows = statement.query([])?;
    let Some(row) = rows.next()? else {
        return Ok((None, None));
    };
    let blob: Vec<u8> = row.get(0)?;
    let fields = decode_fields(&blob)?;
    let time = bytes(&fields, 2)
        .map(parse_timestamp)
        .transpose()?
        .flatten();
    let metadata = bytes(&fields, 1)
        .map(decode_fields)
        .transpose()?
        .unwrap_or_default();
    let workspace = text(&metadata, 1).map(|uri| {
        let raw = uri.strip_prefix("file://").unwrap_or(&uri);
        #[cfg(windows)]
        let raw = if raw.as_bytes().get(2) == Some(&b':') {
            raw.strip_prefix('/').unwrap_or(raw)
        } else {
            raw
        };
        PathBuf::from(raw)
    });
    Ok((time, workspace))
}

/// An identity can connect mirrored observations, but a reused message id
/// cannot connect two distinct provider responses (independent retries).
fn compatible(left: &BTreeSet<String>, right: &BTreeSet<String>) -> bool {
    ["r:", "p:"].into_iter().all(|prefix| {
        let a: BTreeSet<_> = left.iter().filter(|id| id.starts_with(prefix)).collect();
        let b: BTreeSet<_> = right.iter().filter(|id| id.starts_with(prefix)).collect();
        a.is_empty() || b.is_empty() || a == b
    })
}

pub(super) fn normalize(
    mut observations: Vec<Observation>,
    issues: &mut HashMap<SourceKind, ParseIssues>,
) -> Result<Vec<UsageEvent>> {
    // Strong observations come first, making a message-only observation that
    // could belong to multiple retries fail explicitly rather than join them.
    observations.sort_by(|a, b| {
        let strength = |o: &Observation| {
            o.usage
                .identities
                .iter()
                .filter(|id| !id.starts_with("m:"))
                .count()
        };
        strength(b)
            .cmp(&strength(a))
            .then(a.source.as_str().cmp(b.source.as_str()))
            .then(a.path_hash.cmp(&b.path_hash))
            .then(a.location.cmp(&b.location))
    });
    let mut groups: Vec<Vec<Observation>> = Vec::new();
    let mut identities: Vec<BTreeSet<String>> = Vec::new();
    let mut index: HashMap<String, BTreeSet<usize>> = HashMap::new();
    for observation in observations {
        let candidates: BTreeSet<usize> = observation
            .usage
            .identities
            .iter()
            .filter_map(|id| index.get(id))
            .flat_map(|groups| groups.iter().copied())
            .filter(|group| {
                !groups[*group].is_empty()
                    && compatible(&identities[*group], &observation.usage.identities)
            })
            .collect();
        if candidates.len() > 1 {
            let first = *candidates.first().expect("nonempty");
            if candidates
                .iter()
                .any(|other| !compatible(&identities[first], &identities[*other]))
            {
                bail!("Antigravity ambiguous identity bridge; history preserved");
            }
        }
        let group = candidates.first().copied().unwrap_or_else(|| {
            groups.push(Vec::new());
            identities.push(BTreeSet::new());
            groups.len() - 1
        });
        for other in candidates.iter().copied().filter(|other| *other != group) {
            let moved = std::mem::take(&mut groups[other]);
            groups[group].extend(moved);
            let moved_ids = std::mem::take(&mut identities[other]);
            identities[group].extend(moved_ids);
        }
        identities[group].extend(observation.usage.identities.iter().cloned());
        groups[group].push(observation);
        for id in &identities[group] {
            index.entry(id.clone()).or_default().insert(group);
        }
    }

    let mut resolver = ProjectResolver::default();
    let mut events = Vec::new();
    for (mut group, ids) in groups.into_iter().zip(identities) {
        if group.is_empty() {
            continue;
        }
        let explicit_cli = group
            .iter()
            .any(|o| o.product_proven && o.source == SourceKind::Antigravity);
        let explicit_ide = group
            .iter()
            .any(|o| o.product_proven && o.source == SourceKind::AntigravityIde);
        if explicit_cli && explicit_ide {
            bail!("Antigravity conflicting native product ownership; history preserved");
        }
        let source = if explicit_ide {
            SourceKind::AntigravityIde
        } else if explicit_cli || group.iter().any(|o| o.source == SourceKind::Antigravity) {
            SourceKind::Antigravity
        } else {
            SourceKind::AntigravityIde
        };
        group.sort_by(|a, b| {
            let rank = |o: &Observation| {
                (
                    o.usage.output_total == o.usage.visible.saturating_add(o.usage.thinking),
                    o.usage.completeness,
                    o.model.is_some(),
                    o.time.map(|time| time.0).unwrap_or(0),
                )
            };
            rank(b)
                .cmp(&rank(a))
                .then(a.path_hash.cmp(&b.path_hash))
                .then(a.location.cmp(&b.location))
        });
        let chosen = &group[0];
        let issue = issues.entry(source).or_default();
        let tuple = |usage: &Usage| {
            (
                usage.input,
                usage.cache_write,
                usage.cache_read,
                usage.output_total,
                usage.visible,
                usage.thinking,
            )
        };
        if group
            .iter()
            .any(|o| tuple(&o.usage) != tuple(&chosen.usage))
        {
            issue.record(
                source,
                &chosen.path_hash,
                0,
                ParseIssueKind::AccountingAnomaly,
                "conflicting_usage_observations",
            );
            bail!("Antigravity conflicting request usage; history preserved");
        }
        let usage = &chosen.usage;
        if usage.output_total != usage.visible.saturating_add(usage.thinking) {
            // Partial/error usage lacks a trustworthy disjoint-channel breakdown.
            bail!("Antigravity output channel mismatch; history preserved");
        }
        let time = group
            .iter()
            .filter_map(|o| o.time)
            .max_by_key(|(rank, time)| (*rank, std::cmp::Reverse(*time)));
        let Some((_, time)) = time else {
            bail!("Antigravity usage has no typed timestamp; history preserved");
        };
        let Some(timestamp) = DateTime::<Utc>::from_timestamp_millis(time) else {
            bail!("Antigravity invalid typed timestamp");
        };
        let event_at = timestamp.to_rfc3339();
        let hour_start = bucket_start_from_rfc3339(&event_at)
            .ok_or_else(|| anyhow::anyhow!("invalid Antigravity bucket timestamp"))?;
        let model = chosen
            .model
            .clone()
            .or_else(|| group.iter().find_map(|o| o.model.clone()))
            .map(|model| normalize_model(Some(&model)))
            .unwrap_or_else(|| {
                usage
                    .model_id
                    .map(|id| format!("antigravity-model-{id}"))
                    .unwrap_or_else(|| "antigravity-unknown".to_owned())
            });
        let identity = ids
            .iter()
            .find(|id| id.starts_with("r:"))
            .or_else(|| ids.iter().find(|id| id.starts_with("p:")))
            .or_else(|| ids.first())
            .cloned()
            .unwrap_or_else(|| {
                issue.record(
                    source,
                    &chosen.path_hash,
                    0,
                    ParseIssueKind::AccountingAnomaly,
                    "missing_request_identity",
                );
                format!("file:{}:{}", chosen.path_hash, chosen.location)
            });
        let convert = |value: u64| value.min(i64::MAX as u64) as i64;
        let input = convert(usage.input);
        let cache_read = convert(usage.cache_read);
        let cache_write = convert(usage.cache_write);
        let visible = convert(usage.visible);
        let thinking = convert(usage.thinking);
        let project = chosen
            .workspace
            .as_deref()
            .and_then(|path| resolver.resolve(path).ok().flatten());
        events.push(UsageEvent {
            event_key: format!("antigravity-request:{}", hash_string(&identity)),
            source,
            provider_label: String::new(),
            model,
            event_at,
            hour_start,
            tokens: UsageTokens {
                input_tokens: input,
                cache_read_tokens: cache_read,
                cache_creation_tokens: cache_write,
                output_tokens: visible,
                reasoning_output_tokens: thinking,
                total_tokens: input
                    .saturating_add(cache_read)
                    .saturating_add(cache_write)
                    .saturating_add(visible)
                    .saturating_add(thinking),
            },
            project,
            session: Some(SessionInfo {
                session_id: chosen.path_hash.clone(),
                session_label: None,
                source_path_hash: Some(super::group_hash(source)),
            }),
            source_cost: None,
        });
    }
    events.sort_by(|a, b| a.event_key.cmp(&b.event_key));
    Ok(events)
}

#[derive(Debug)]
enum Value {
    Number(u64),
    Bytes(Vec<u8>),
}
type Fields = BTreeMap<u32, Vec<Value>>;
fn check_types(fields: &Fields, numeric: &[u32], messages: &[u32]) -> Result<()> {
    if numeric.iter().any(|field| {
        fields.get(field).is_some_and(|values| {
            values
                .iter()
                .any(|value| !matches!(value, Value::Number(_)))
        })
    }) || messages.iter().any(|field| {
        fields
            .get(field)
            .is_some_and(|values| values.iter().any(|value| !matches!(value, Value::Bytes(_))))
    }) {
        bail!("unexpected Antigravity protobuf field type");
    }
    Ok(())
}
fn number(fields: &Fields, field: u32) -> Option<u64> {
    match fields.get(&field)?.last()? {
        Value::Number(value) => Some(*value),
        _ => None,
    }
}
fn bytes(fields: &Fields, field: u32) -> Option<&[u8]> {
    match fields.get(&field)?.last()? {
        Value::Bytes(value) => Some(value),
        _ => None,
    }
}
fn all_bytes(fields: &Fields, field: u32) -> impl Iterator<Item = &[u8]> {
    fields
        .get(&field)
        .into_iter()
        .flatten()
        .filter_map(|value| match value {
            Value::Bytes(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
}
fn text(fields: &Fields, field: u32) -> Option<String> {
    String::from_utf8(bytes(fields, field)?.to_vec())
        .ok()
        .filter(|text| !text.is_empty())
}
fn parse_timestamp(blob: &[u8]) -> Result<Option<i64>> {
    let fields = decode_fields(blob)?;
    let Some(seconds) = number(&fields, 1).and_then(|value| i64::try_from(value).ok()) else {
        return Ok(None);
    };
    let nanos = number(&fields, 2).unwrap_or(0);
    if nanos >= 1_000_000_000 {
        bail!("invalid protobuf timestamp nanos");
    }
    Ok(DateTime::<Utc>::from_timestamp(seconds, nanos as u32).map(|time| time.timestamp_millis()))
}
fn varint(blob: &[u8], pos: &mut usize) -> Result<u64> {
    let mut value = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = *blob
            .get(*pos)
            .ok_or_else(|| anyhow::anyhow!("truncated protobuf varint"))?;
        *pos += 1;
        if shift == 63 && byte > 1 {
            bail!("protobuf varint overflow");
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    bail!("protobuf varint overflow")
}
fn decode_fields(blob: &[u8]) -> Result<Fields> {
    let mut fields = Fields::new();
    let mut pos = 0;
    while pos < blob.len() {
        let key = varint(blob, &mut pos)?;
        let field = u32::try_from(key >> 3)?;
        if field == 0 {
            bail!("invalid protobuf field zero");
        }
        let value = match key & 7 {
            0 => Some(Value::Number(varint(blob, &mut pos)?)),
            2 => {
                let len = usize::try_from(varint(blob, &mut pos)?)?;
                let end = pos
                    .checked_add(len)
                    .filter(|end| *end <= blob.len())
                    .ok_or_else(|| anyhow::anyhow!("truncated protobuf bytes"))?;
                let value = Value::Bytes(blob[pos..end].to_vec());
                pos = end;
                Some(value)
            }
            wire @ (1 | 5) => {
                pos = pos
                    .checked_add(if wire == 1 { 8 } else { 4 })
                    .filter(|end| *end <= blob.len())
                    .ok_or_else(|| anyhow::anyhow!("truncated protobuf fixed width"))?;
                None
            }
            _ => bail!("unsupported protobuf wire type"),
        };
        if let Some(value) = value {
            fields.entry(field).or_default().push(value);
        }
    }
    Ok(fields)
}
