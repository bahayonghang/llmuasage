import sqlite3
from pathlib import Path

con = sqlite3.connect(
    f"file:{(Path.home() / '.llmusage' / 'llmusage.db').as_posix()}?mode=ro",
    uri=True,
)
print(
    "dsh empty provider",
    con.execute(
        "SELECT COUNT(*) FROM usage_event WHERE source='deepseek_harness' AND COALESCE(provider_label,'')=''"
    ).fetchone()[0],
)
print(
    "dsh total",
    con.execute("SELECT COUNT(*) FROM usage_event WHERE source='deepseek_harness'").fetchone()[0],
)
print(
    "omp source_file",
    con.execute("SELECT COUNT(*) FROM source_file WHERE source='omp'").fetchone()[0],
)
print(
    "pi source_file",
    con.execute("SELECT COUNT(*) FROM source_file WHERE source='pi'").fetchone()[0],
)
print(
    "omp distinct project_hash",
    con.execute(
        "SELECT COUNT(DISTINCT project_hash) FROM usage_event WHERE source='omp'"
    ).fetchone()[0],
)
