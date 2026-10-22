"""Capture actual headless CLI examples with isolated synthetic Codex input."""
from pathlib import Path
import json
import os
import subprocess
import tempfile

ROOT = next(parent for parent in Path(__file__).resolve().parents
            if (parent / 'Cargo.toml').is_file() and (parent / '.trellis').is_dir())
OUT = Path(__file__).resolve().parent
EXE = ROOT / 'output/s-language-rust-target/debug/llmusage.exe'
results = []
with tempfile.TemporaryDirectory(prefix='language-doc-fixture-', dir=ROOT / 'output') as temp:
    base = Path(temp)
    for language in ('en', 'zh'):
        home = base / language
        profile = home / 'profile'
        codex = home / '.codex'
        sessions = codex / 'sessions/2026/04/22'
        sessions.mkdir(parents=True)
        usage = dict(input_tokens=123, cached_input_tokens=0, output_tokens=0,
                     reasoning_output_tokens=0, total_tokens=123)
        rows = [
            dict(type='session_meta', payload=dict(model='gpt-5', cwd=str(home / 'demo'))),
            dict(timestamp='2026-04-22T01:12:00Z', payload=dict(type='token_count', info=dict(
                last_token_usage=usage, total_token_usage=usage))),
        ]
        fixture = '\n'.join(json.dumps(row, separators=(',', ':')) for row in rows) + '\n'
        fixture_path = sessions / 'rollout-table.jsonl'
        fixture_path.write_bytes(fixture.encode('utf-8'))
        env = os.environ.copy()
        env.update(HOME=str(home), USERPROFILE=str(home), CODEX_HOME=str(codex),
                   LLMUSAGE_HOME=str(profile), LLMUSAGE_PRICING_REFRESH='off',
                   RUST_LOG='off', NO_COLOR='1', COLUMNS='200')
        for key in ('OPENCODE_DB', 'KIMI_CODE_HOME', 'PI_AGENT_DIR', 'GROK_HOME',
                    'ZCODE_HOME', 'GEMINI_CLI_HOME', 'DSH_HOME'):
            env.pop(key, None)
        def run(*args):
            result = subprocess.run([str(EXE), '--home', str(profile), *args],
                                    cwd=home, env=env, capture_output=True)
            if result.returncode:
                raise RuntimeError(result.stderr.decode('utf-8', errors='replace'))
            return result
        run('lang', language)
        output = run('sync', '--source', 'codex')
        stdout = output.stdout.decode('utf-8')
        (OUT / f'fixture-sync.{language}.txt').write_text(stdout, encoding='utf-8')
        stderr = output.stderr.decode('utf-8')
        # Progress may contain this explicitly synthetic temporary path.
        (OUT / f'fixture-sync.{language}.stderr.txt').write_text(
            stderr.replace(str(base), '<TEMP_FIXTURE>'), encoding='utf-8')
        assert '\x1b' not in stdout
        # Summary columns count events, not token quantities in the fixture.
        assert 'codex' in stdout and f'{fixture_path.stat().st_size} B' in stdout
        results.append(dict(language=language, exit_code=output.returncode,
                            command='llmusage --home <TEMP_PROFILE> sync --source codex',
                            fixture_bytes=len(fixture.encode('utf-8')),
                            tokens=123, mode='headless subprocess; not native terminal'))
(OUT / 'fixture-capture.json').write_text(json.dumps(results, indent=2) + '\n', encoding='utf-8')
print(json.dumps(results, ensure_ascii=False))
