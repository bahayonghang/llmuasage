"""One-time source inventory; edits only append stable tracing fields at existing sites."""
import json
import sys
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
OUT = Path(__file__).parent

def split_tokens(text):
    parts, start, depth, quote, escape = [], 0, 0, False, False
    for i, ch in enumerate(text):
        if quote:
            if escape: escape = False
            elif ch == '\\': escape = True
            elif ch == '"': quote = False
        elif ch == '"': quote = True
        elif ch in '([{': depth += 1
        elif ch in ')]}': depth -= 1
        elif ch == ',' and depth == 0:
            parts.append(text[start:i].strip()); start = i + 1
    parts.append(text[start:].strip())
    return parts

events = []
for path in sorted((ROOT / 'src').rglob('*.rs')):
    source = path.read_text(encoding='utf-8')
    if '--apply' in sys.argv:
        source = re.sub(r'event_code = "[a-z_]+(?:\.[a-z_]+)*\.\d+",\s*', '', source)
        if path.as_posix().endswith('sync/engine.rs'):
            source = source.replace('detail = %warning, ', '')
        if path.as_posix().endswith('codex_tracer/server.rs'):
            source = source.replace('addr = %actual_addr, ', '')
        path.write_text(source, encoding='utf-8')
    mask = re.sub(r'//[^\n]*|/\*.*?\*/|"(?:[^"\\]|\\.)*"', lambda m: ' ' * len(m[0]), source, flags=re.S)
    for index, match in enumerate(re.finditer(r'(?<![\w])(?:tracing::)?(trace|debug|info|warn|error)!\s*\(', mask), 1):
        pos, depth, quoted, escape = match.end(), 1, False, False
        while depth:
            ch = source[pos]
            if quoted:
                if escape: escape = False
                elif ch == '\\': escape = True
                elif ch == '"': quoted = False
            elif ch == '"': quoted = True
            elif ch == '(': depth += 1
            elif ch == ')': depth -= 1
            pos += 1
        body = source[match.end():pos - 1]
        tokens = split_tokens(body)
        message = next((json.loads(re.sub(r'\\\n\s*', '', token)) for token in tokens if re.fullmatch(r'"(?:[^"\\]|\\.)*"', token, re.S)), None)
        rel = path.relative_to(ROOT).as_posix()
        code = rel.removeprefix('src/').removesuffix('.rs').replace('/', '.') + f'.{index:02d}'
        events.append(dict(path=rel, start=match.end(), line=source.count('\n', 0, match.start())+1, code=code, level=match[1], message=message, body=body))
(OUT/'event-inventory.json').write_text(json.dumps(events, ensure_ascii=False, indent=2), encoding='utf-8')
for event in events:
    print(event['code'], json.dumps(event['message'], ensure_ascii=False))

if '--apply' in sys.argv:
    for rel in sorted({event['path'] for event in events}):
        path = ROOT / rel
        source = path.read_text(encoding='utf-8')
        selected = [event for event in events if event['path'] == rel]
        for event in reversed(selected):
            body = event['body']
            if 'event_code =' in body:
                continue
            prefix = f'event_code = "{event["code"]}", '
            if event['message'] == '{warning}': prefix += 'detail = %warning, '
            if event['message'] == 'Codex Tracer dashboard listening on {}': prefix += 'addr = %actual_addr, '
            position = event['start']
            source = source[:position] + prefix + source[position:]
        path.write_text(source, encoding='utf-8')
