"""Read only accounting and inventory metadata; never print message content."""
import argparse
import collections
import datetime
import json
import os
from pathlib import Path
import sqlite3

parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path)
args = parser.parse_args()
db = Path.home() / '.llmusage' / 'llmusage.db'
wal = Path(str(db) + '-wal')
if wal.exists():
    raise SystemExit('Probe refused: live WAL exists; obtain a consistent isolated snapshot first.')
before = (db.stat().st_size, db.stat().st_mtime_ns)
conn = sqlite3.connect(db.as_uri() + '?mode=ro&immutable=1', uri=True)
conn.row_factory = sqlite3.Row
report = {'observed_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'database_bytes': before[0], 'method': 'SQLite mode=ro&immutable=1; no WAL',
          'accounting_markers': [dict(row) for row in conn.execute(
              "SELECT key,value FROM meta WHERE key LIKE '%token_accounting%' AND key LIKE '%antigravity%'")],
          'inventory': {}}
base = Path(os.environ.get('GEMINI_CLI_HOME', str(Path.home() / '.gemini')))
for source, folder in [('antigravity', 'antigravity-cli'), ('antigravity_ide', 'antigravity-ide')]:
    rows = list(conn.execute(
        "SELECT file_path,state FROM source_file WHERE source=? AND host_id='local'", (source,)))
    cursors = list(conn.execute(
        "SELECT file_path FROM source_cursor WHERE source=? AND host_id='local'", (source,)))
    tracked = {row['file_path'] for row in rows + cursors if row['file_path']}
    missing = [Path(path) for path in tracked if not Path(path).exists()]
    root = base / folder / 'conversations'
    report['inventory'][source] = {
        'tracked_rows': len(rows), 'cursor_rows': len(cursors), 'union_paths': len(tracked),
        'persisted_states': dict(collections.Counter(row['state'] for row in rows)),
        'missing_now': len(missing),
        'missing_suffixes': dict(collections.Counter(path.suffix for path in missing)),
        'tracked_suffixes': dict(collections.Counter(Path(path).suffix for path in tracked)),
        'root_db_files_now': len(list(root.glob('*.db'))) if root.exists() else 0,
        'root_exists': root.exists(), 'custom_gemini_root': 'GEMINI_CLI_HOME' in os.environ,
    }
report['latest_status'] = [dict(row) for row in conn.execute(
    "SELECT source,files_processed,changed_files,events_seen,events_replayed,events_inserted,stored_events,parse_ms,write_ms,bytes_scanned,updated_at FROM source_sync_status WHERE host_id='local' ORDER BY source")]
report['latest_sync_run'] = dict(conn.execute(
    "SELECT command,status,started_at,finished_at,duration_ms FROM run_log WHERE command='sync' ORDER BY id DESC LIMIT 1").fetchone())
report['totals'] = {key: sum(row[key] for row in report['latest_status'])
                    for key in ['files_processed','changed_files','events_seen','events_inserted','stored_events','parse_ms','write_ms','bytes_scanned']}
conn.close()
if before != (db.stat().st_size, db.stat().st_mtime_ns) or wal.exists():
    raise SystemExit('Probe refused: database changed during inspection; discard observations.')
report['database_metadata_stable'] = True
content = json.dumps(report, indent=2, ensure_ascii=False) + '\n'
if args.output:
    args.output.write_text(content, encoding='utf-8')
print(content)
