# Web Server Contracts

## Scenario: Browser dashboard listener and launch policy

### 1. Scope / Trigger

- Apply this contract when changing `llmusage serve`, `web::serve`, listener selection,
  browser launching, SSH behavior, or dashboard network exposure.
- The live routes, query layer, token-accounting startup repair, and port-probe order are
  outside this contract unless a change explicitly targets them.

### 2. Signatures

```text
Commands::Serve {
    port: Option<u16>,
    public: bool,
    no_open: bool,
}

commands::serve::run(app, port) -> Result<()>        // compatibility wrapper
commands::serve::run_with_options(app, port, public, no_open) -> Result<()>
web::serve(store, preferred_port) -> Result<SocketAddr> // loopback compatibility wrapper
web::serve_on(store, preferred_port, bind_ip) -> Result<SocketAddr>
```

### 3. Contracts

- Without `--public`, `serve` binds `127.0.0.1`; `--public` is the only CLI opt-in that
  binds `0.0.0.0`.
- `--no-open` suppresses automatic browser launching. A non-empty `SSH_CONNECTION` or
  `SSH_TTY` also suppresses it, without invoking the platform launcher.
- The public `web::serve` and `commands::serve::run` wrappers retain their loopback and
  no-extra-option semantics for embedding compatibility.
- `0.0.0.0` is a bind address, never a browser URL. Browser/local output uses
  `http://127.0.0.1:<port>`; public output uses the placeholder
  `http://<server-host-or-ip>:<port>`.
- `--public` exposes the dashboard and JSON API without authentication or TLS. Output and
  documentation must tell users to use a firewall, SSH tunnel, or authenticated reverse proxy.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Default invocation | Bind loopback and retain existing port order `37421/37422/37423/0` |
| `--public` | Bind `0.0.0.0` using the same port order and print the exposure warning |
| `--no-open` | Do not invoke a browser launcher |
| SSH environment | Do not invoke a browser launcher or log its expected failure; print an access hint |
| Non-SSH launcher failure | Warn but keep the server running |
| Public listener viewed locally | Open/use loopback URL, never `http://0.0.0.0:<port>` |

### 5. Good / Base / Bad Cases

- Good: `llmusage serve --public --no-open --port 37421` listens on all IPv4 interfaces,
  prints a server-host placeholder, and explains the authentication/TLS boundary.
- Base: `llmusage serve` remains local-only and attempts to open the default browser.
- Bad: making `0.0.0.0` the default listener, or passing `http://0.0.0.0:<port>` to the
  browser launcher.

### 6. Tests Required

- CLI parsing asserts default values plus `--public` and `--no-open`.
- Browser-policy tests cover local, `--no-open`, `SSH_CONNECTION`, and `SSH_TTY` paths.
- URL tests assert a public listener still uses the loopback browser URL and a remote-host
  placeholder for instructions.
- Web tests bind port `0` to `0.0.0.0`, assert the returned address, and fetch `/` through
  loopback to preserve the live-route contract.
- CLI help and the English/Chinese Dashboard, Safety, and CLI-reference docs must mention
  the default, flags, SSH behavior, and unauthenticated/TLS-free boundary.

### 7. Wrong vs Correct

#### Wrong

```rust
let dashboard_url = format!("http://{addr}"); // addr may be 0.0.0.0:37421
open_dashboard_in_browser(&dashboard_url)?;
```

#### Correct

```rust
let local_url = format!("http://127.0.0.1:{}", addr.port());
let remote_hint = format!("http://<server-host-or-ip>:{}", addr.port());
```

The listener address controls where the process accepts connections; the browser URL must be
reachable from the browser's own network namespace.
