import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function run(args) {
  console.log(`+ ${process.execPath} ${args.join(' ')}`);
  const result = spawnSync(process.execPath, args, {
    cwd: root,
    stdio: 'inherit',
  });
  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }
  if (result.signal) {
    console.error(`error: node exited from signal ${result.signal}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const testsDir = path.join(root, 'scripts', 'tests');
let entries;
try {
  entries = fs.readdirSync(testsDir, { withFileTypes: true });
} catch (err) {
  console.error(`error: cannot read ${testsDir}: ${err.message}`);
  process.exit(1);
}

const testFiles = entries
  .filter((entry) => entry.isFile() && entry.name.endsWith('.test.mjs'))
  .map((entry) => entry.name)
  .sort()
  .map((name) => `scripts/tests/${name}`);

if (testFiles.length === 0) {
  console.error('error: no scripts/tests/*.test.mjs files found');
  process.exit(1);
}

run(['--check', 'scripts/benchmark-dashboard-range.mjs']);
run(['--check', 'scripts/benchmark-top-sessions.mjs']);
run(['--test', ...testFiles]);
