set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    just --list

install:
    cargo install --path . --locked --force

serve:
    cargo run -- serve

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

    cargo metadata --format-version 1 --no-deps | Out-Null

ci:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features -- --test-threads=1
    $env:RUSTDOCFLAGS = "-D warnings"; cargo doc --no-deps
    node --check scripts/benchmark-dashboard-range.mjs
    node --test scripts/tests/dashboard-fetch.test.mjs
    node --test scripts/tests/dashboard-bootstrap-watchdog.test.mjs
    node --test scripts/tests/dashboard-load-state.test.mjs
    node --test scripts/tests/dashboard-render-lifecycle.test.mjs
    npm --prefix docs run docs:build
