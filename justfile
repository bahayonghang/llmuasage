set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    just --list

install:
    cargo +1.97.0 install --path . --locked --force

serve:
    cargo run -- serve

desktop-dev:
    cmd.exe /c 'desktop\node_modules\.bin\tauri.cmd dev'

alias tdev := desktop-dev

desktop-test:
    npm --prefix desktop test
    cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1

desktop-build:
    npm --prefix desktop install
    cmd.exe /c 'set CI=true && desktop\node_modules\.bin\tauri.cmd build --ci --no-sign --bundles nsis'

[script("powershell.exe", "-NoLogo", "-NoProfile", "-File")]
tinstall: desktop-build
    $ErrorActionPreference = "Stop"
    $nsisDir = "desktop\src-tauri\target\release\bundle\nsis"
    if (-not (Test-Path $nsisDir)) {
        throw "NSIS output directory not found: $nsisDir"
    }
    $installer = Get-ChildItem -Path $nsisDir -Filter *.exe |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        throw "No NSIS installer found in $nsisDir"
    }
    $process = Start-Process -FilePath $installer.FullName -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "NSIS installer exited with code $($process.ExitCode)"
    }

build:
    cargo build --release --locked
    npm --prefix docs run docs:build

docs:
    npm --prefix docs run docs:dev

[script("powershell.exe", "-NoLogo", "-NoProfile", "-File")]
version-sync version:
    $ErrorActionPreference = "Stop"
    $version = "{{version}}"
    $semverPattern = '\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?'
    if ($version -notmatch "^$semverPattern$") {
        throw "Invalid semantic version: $version"
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $cargoTomlPath = "Cargo.toml"
    $cargoToml = [System.IO.File]::ReadAllText($cargoTomlPath)
    $cargoVersionPattern = '(?m)(?<=^version = ")' + $semverPattern + '(?=")'
    if (-not [regex]::IsMatch($cargoToml, $cargoVersionPattern)) {
        throw "Could not read package.version from Cargo.toml"
    }
    $cargoToml = [regex]::Replace($cargoToml, $cargoVersionPattern, $version, 1)
    [System.IO.File]::WriteAllText($cargoTomlPath, $cargoToml, $utf8NoBom)

    $targets = @(
        @{ Path = "README.md"; Prefix = '> Current crate version: `'; Suffix = '`.' },
        @{ Path = "README.zh-CN.md"; Prefix = '> 当前 crate 版本：`'; Suffix = '`。' },
        @{ Path = "docs/index.md"; Prefix = '- Version `'; Suffix = '`.' },
        @{ Path = "docs/zh/index.md"; Prefix = '- 版本：`'; Suffix = '`。' },
        @{ Path = "docs/reference/cli.md"; Prefix = 'for version `'; Suffix = '`.' },
        @{ Path = "docs/zh/reference/cli.md"; Prefix = '本页按版本 `'; Suffix = '` 的' }
    )
    foreach ($target in $targets) {
        $content = [System.IO.File]::ReadAllText($target.Path)
        $pattern = '(?<=' + [regex]::Escape($target.Prefix) + ')' + $semverPattern + '(?=' + [regex]::Escape($target.Suffix) + ')'
        if (-not [regex]::IsMatch($content, $pattern)) {
            throw "Could not find version reference in $($target.Path)"
        }
        $content = [regex]::Replace($content, $pattern, $version, 1)
        [System.IO.File]::WriteAllText($target.Path, $content, $utf8NoBom)
    }

    cargo update --offline --package llmusage

ci:
    cargo update --offline --package llmusage
    python scripts/check-ci-gate.py --self-test
    python scripts/check-ci-gate.py
    python scripts/ci-rust.py
    node --check scripts/benchmark-dashboard-range.mjs
    node --check scripts/benchmark-top-sessions.mjs
    node --test scripts/tests/benchmark-top-sessions.test.mjs
    node --test scripts/tests/dashboard-fetch.test.mjs
    node --test scripts/tests/dashboard-logs-viewer.test.mjs
    node --test scripts/tests/dashboard-bootstrap-watchdog.test.mjs
    node --test scripts/tests/dashboard-load-state.test.mjs
    node --test scripts/tests/dashboard-render-lifecycle.test.mjs
    npm --prefix docs run docs:build
