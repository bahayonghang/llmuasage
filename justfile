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

ci:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test -- --test-threads=1
    npm --prefix docs run docs:build
