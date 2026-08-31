# Breaking-change notes

## reqwest 0.12.28 → 0.13.4

Source: https://github.com/seanmonstar/reqwest/releases/tag/v0.13.0

Current usage (`Cargo.toml` + `src/subscription/http.rs` and provider clients):

```toml
reqwest = { version = "0.12", default-features = false, features = [
    "rustls-tls",
    "json",
    "http2",
] }
```

Code builds a `Client` with timeout + user-agent, then `response.json().await`. No `query()`, `form()`, `use_rustls_tls()`, or custom DNS resolver.

0.13 breaking points that apply here:

| Change | Impact |
| --- | --- |
| Feature `rustls-tls` renamed to `rustls` | Must edit `Cargo.toml` features; compile-fail otherwise |
| rustls is the default TLS backend | We already disable default features and enable rustls explicitly — keep that |
| Default rustls crypto is aws-lc, not ring | Possible extra native build deps on some hosts; test Windows/Linux/macOS CI |
| `query` / `form` are optional features off by default | No code uses them |
| Long-deprecated features removed (`trust-dns`, etc.) | We never enabled them |

Migration: bump version to `0.13`, rename feature `rustls-tls` → `rustls`, keep `json` + `http2` + `default-features = false`. Then `cargo update -p reqwest` and run the full gate plus subscription-focused tests.

Do not take this in the same commit as the MSRV-compatible lockfile refresh.

## syn 2.0.119 → 3.0.4 (dev-only)

Architecture tests in `tests/architecture/main.rs` use `syn::{Item, ItemMod, Path, UseTree, visit::Visit, parse_file, spanned::Spanned}`.

syn 3 rewrites visit/fold, `Type::BareFn` → `Type::FnPtr`, `Punctuated::pop` return type, and several enum shapes. The lockfile already has transitive syn 3.0.3 (serde/thiserror-impl paths); our tests still compile against syn 2.

Value is low (test-only AST walker). Cost is high (visit API edits, risk of false architecture failures). Keep on syn 2 unless a later crate forces it.

## VitePress 2

`vitepress@2.0.0-alpha.19` exists; 1.6.4 is latest stable. Alpha is out of scope. Residual vite/esbuild advisories stay until 1.6.5 or 2.0 stable can pull vite ≥ 6.4.3.
