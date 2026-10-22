from pathlib import Path
import json
import re
root = Path(__file__).resolve().parents[4]
test_codes = {f'commands.sync_progress.{i:02}' for i in range(1, 6)} | {'runtime.logging.01', 'store.sync_writer.profiling.01', 'store.sync_writer.profiling.02'}
for rel in ['src/commands/sync_progress.rs', 'src/runtime/logging.rs', 'src/store/sync_writer/profiling.rs']:
    path = root / rel
    source = path.read_text(encoding='utf-8')
    for code in test_codes:
        source = source.replace(f'event_code = "{code}", ', '')
    path.write_text(source, encoding='utf-8')
path = root/'src/common/i18n/logging.rs'
source = path.read_text(encoding='utf-8')
source = '\n'.join(line for line in source.splitlines() if not any(f'code: "{code}"' in line for code in test_codes))+'\n'
source = source.replace('english: "Remote import warning", chinese: "远端导入警告"', 'english: "Remote import skipped non-JSON shard lines", chinese: "远端导入已跳过非 JSON 分片行"')
path.write_text(source, encoding='utf-8')
path = Path(__file__).parent/'event-inventory.json'
events = json.loads(path.read_text(encoding='utf-8'))
for event in events: event['test_only'] = event['code'] in test_codes
path.write_text(json.dumps(events, ensure_ascii=False, indent=2), encoding='utf-8')
print(f'{len(events)-len(test_codes)} production sites; {len(test_codes)} unchanged test sites')
