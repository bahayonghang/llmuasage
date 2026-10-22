from pathlib import Path
import re

path = Path('src/commands/sync_progress.rs')
source = path.read_text(encoding='utf-8')
mask = re.sub(r'//[^\n]*|/\*.*?\*/|"(?:[^"\\]|\\.)*"', lambda m: ' '*len(m[0]), source, flags=re.S)
edits = []
for match in re.finditer(r'#\[test\]\s*fn\s+\w+\s*\([^)]*\)[^{]*\{', mask):
    start = match.end(); pos = start; depth = 1
    while depth:
        if mask[pos] == '{': depth += 1
        elif mask[pos] == '}': depth -= 1
        pos += 1
    edits.extend([(start, '\n        i18n::with_language(Language::Chinese, || {'), (pos-1, '\n        })\n    ')])
for position, value in sorted(edits, reverse=True): source = source[:position]+value+source[position:]
path.write_text(source, encoding='utf-8')

path = Path('src/commands/sync_summary.rs')
source = path.read_text(encoding='utf-8')
for label in ['codex','opencode','TOTAL']:
    source = source.replace(f'.starts_with("{label}")', f'.starts_with("│ {label}")')
source = source.replace('.starts_with("antig…")', '.starts_with("│ anti")')
source = source.replace('total.split_whitespace().collect::<Vec<_>>()', "total.split('│').flat_map(str::split_whitespace).collect::<Vec<_>>()")
source = source.replace('line.contains("parse issues:")', 'line.contains("Parse faults:")')
source = source.replace('l.contains("parse issues:")', 'l.contains("Parse faults:")')
source = source.replace('"  parse issues: malformed=2 oversized=1 skipped=3 accounting=4"', '"  Parse faults: malformed=2 oversized=1"')
source = source.replace('strip_ansi(line).contains("parse issues:")', 'strip_ansi(line).contains("Information:")')
source = source.replace('assert_eq!(table_data_lines.len(), summary.sources.len());', 'assert_eq!(table_data_lines.len(), summary.sources.len() + 2);')
source = source.replace('table_lines.clone().all(|line| line.chars().count() <= 60)', 'table_lines.clone().all(|line| display_width(line) == display_width(&row))')
path.write_text(source, encoding='utf-8')
