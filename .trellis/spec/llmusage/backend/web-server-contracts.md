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
web::bind_server(store, preferred_port, bind_ip, write_exposure) -> Result<BoundWebServer>
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
- `--public` exposes a reduced aggregate dashboard without authentication or TLS. Output and
  documentation must distinguish its explicit read allowlist from the full loopback API and tell
  users to use a firewall or authenticated reverse proxy. An SSH tunnel to loopback is the path to
  the full local dashboard.
- The CLI owns and supervises `BoundWebServer` until Ctrl+C. An early server return, task error,
  or panic fails the `serve` command; graceful shutdown has a finite deadline.
- The `serve` run-log record spans the full listener session. Clean shutdown records success,
  bind/server failures record failed, and a later serve invocation recovers stale running records.
- Live and snapshot shells install an inline `claim -> ready` bootstrap watchdog before the module
  entry. Live mode probes only `/` with a finite deadline; snapshot mode never claims a local
  service stopped. Every watchdog terminal state replaces the static sync-center placeholder.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Default invocation | Bind loopback and retain existing port order `37421/37422/37423/0` |
| `--public` | Bind `0.0.0.0`, select the public read allowlist, and print the exposure warning |
| `--no-open` | Do not invoke a browser launcher |
| SSH environment | Do not invoke a browser launcher or log its expected failure; print an access hint |
| Non-SSH launcher failure | Warn but keep the server running |
| Public listener viewed locally | Open/use loopback URL, never `http://0.0.0.0:<port>` |
| Explicit port occupied | Return an error containing the attempted address and bind reason; never panic |
| Server task exits early | Propagate the terminal result and finish the tracked run as failed |
| Module graph or initial app startup fails | Replace loading within the watchdog deadline and offer page reload |

### 5. Good / Base / Bad Cases

- Good: `llmusage serve --public --no-open --port 37421` listens on all IPv4 interfaces,
  prints a server-host placeholder, and explains both the reduced read surface and the
  authentication/TLS boundary.
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
- Real TCP tests assert every loopback-only read/write route is absent from the public router and
  that the public dashboard projection contains no path, raw-log, error, diagnostic, or job fields.
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

## Scenario: Public read security boundary

### 1. Scope / Trigger

- Apply this contract when adding or changing browser routes, dashboard DTOs, diagnostics, runtime
  logs, project data, cursor health, or sync-job polling.
- The public listener is an unauthenticated aggregate reporting surface. It is not a remote
  administration or troubleshooting API.

### 2. Signatures

```text
loopback_router() -> Router<WebState>
public_router() -> Router<WebState>

GET /api/dashboard (public) -> PublicDashboardPayload
GET /api/health    (public) -> { status: "ok", exposure: "public_read_only" }
```

### 3. Contracts

- `loopback_router` retains the full local API, including logs, diagnostics, project breakdowns,
  job reads, behavior/Explorer reads, and mutation routes guarded by the real TCP peer.
- `public_router` is built from a positive allowlist containing only `/`, `/assets/{*path}`,
  `/api/dashboard`, and `/api/health`. Loopback routes are not merged into it.
- The public dashboard DTO copies an explicit set of aggregate overview, trend, model, source, and
  cost fields. It never serializes query-layer project, health, diagnostics, sync-command-center,
  log, or job DTOs wholesale.
- The public dashboard ignores `project` and `project_hash` query selectors so omitted project
  identities cannot be inferred by probing aggregate totals. Other aggregate filters remain
  available.
- Public `projects` is always empty, `health` contains no local details, and `diagnostics` only says
  that local details are unavailable. The health endpoint is a fixed DTO and performs no local
  diagnostic query.
- Public route selection depends only on the server exposure configured before binding. `Host`,
  `Origin`, `Forwarded`, and `X-Forwarded-For` are not authentication or route-selection inputs.
- Adding remote diagnostics requires a new explicit opt-in plus authentication. It must not expand
  the default public allowlist.
- API failures return the generic client error contract. Paths, SQL, raw logs, and database error
  detail remain server-side structured logs only.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Public aggregate dashboard | 200 with only the typed aggregate projection |
| Public minimal health | 200 with fixed `status` and `exposure` fields |
| Public logs/diagnostics/projects/job/detail reads | Route absent: 404/405 |
| Public behavior/Explorer/legacy section reads | Route absent: 404/405 |
| Public mutation request | Route absent: 404/405 |
| Loopback sensitive read | Existing handler and payload remain available |
| Query or serialization failure | Generic 500 response without internal detail |

### 5. Good / Base / Bad Cases

- Good: public clients can compare aggregate usage while local paths, project labels, run failures,
  cursor details and job ids never cross the response boundary. Integration
  health is absent from both local and public dashboard payloads.
- Base: loopback users retain the full local dashboard and API behavior.
- Bad: building public mode by starting with `loopback_router`, then trying to deny sensitive paths
  with headers or request-time guards.

### 6. Tests Required

- A real TCP public-listener test enumerates every loopback-only read route and expects 404/405.
- A public payload test seeds Windows/Unix paths, raw JSON-shaped details, SQL text, run errors, and
  job-adjacent state, then recursively rejects forbidden keys and values.
- The public route inventory test pins the complete positive allowlist so additions require an
  intentional security review.
- Loopback regression tests cover logs, diagnostics, projects, and job polling in addition to the
  existing write-route tests.
- English/Chinese README, Dashboard, Safety, and CLI-reference docs state the capability split.

### 7. Wrong vs Correct

#### Wrong

```rust
let app = loopback_router().layer(reject_public_sensitive_reads);
```

#### Correct

```rust
let app = match exposure {
    WriteExposure::LocalOnly => loopback_router(),
    WriteExposure::PublicReadOnly => public_router(),
};
```

The public boundary is the router inventory plus a dedicated DTO projection, not a client-header
policy layered over local diagnostic payloads.

## Scenario: Client-filter-safe embedded module URLs

### 1. Scope / Trigger

- Apply this contract when adding or renaming browser-visible embedded assets, changing an ES
  module import, or diagnosing `net::ERR_BLOCKED_BY_CLIENT` before application bootstrap.
- Browser-visible asset paths must avoid telemetry/identity terms commonly matched by privacy or
  content filters when the asset does not implement those behaviors.

### 2. Signatures

```text
ASSETS: EmbeddedAsset {
    path: "data/render-key.js",
    mime: "application/javascript; charset=utf-8",
    body: include_str!("data/render-key.js"),
}

GET /assets/data/render-key.js  -> 200 application/javascript
GET /assets/data/fingerprint.js -> 404

stableSerialize(value) -> string
panelFingerprint(value) -> string
```

### 3. Contracts

- `data/render-key.js` is the canonical embedded route and module import for panel render cache
  keys. Live and snapshot module graphs use the same route.
- Renaming the browser-visible path must not change stable serialization, cache-key output, panel
  dirty-check behavior, or the exported `stableSerialize` / `panelFingerprint` symbols.
- Do not serve a compatibility alias at `data/fingerprint.js`. The alias preserves the path that
  client filters block and can hide stale imports.
- An asset that is a top-level dependency of `app.js` is bootstrap-critical: if a client blocks
  its URL, no application code can claim the inline shell watchdog.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Canonical render-key request | HTTP 200 with the embedded JavaScript body |
| Stale fingerprint request | HTTP 404 so stale imports fail deterministically |
| Live or snapshot module graph | Contains `data/render-key.js` and no `fingerprint.js` URL |
| `ERR_BLOCKED_BY_CLIENT` in a user browser | Inspect the failed URL before attributing delay to SQLite or API performance |
| Path-only rename | Serialization and panel fingerprint regression assertions remain unchanged |

### 5. Good / Base / Bad Cases

- Good: a render cache helper is served as `/assets/data/render-key.js`, loads under the user's
  existing content-filter extensions, and preserves its deterministic key output.
- Base: ordinary browser assets use domain-neutral names that describe presentation behavior.
- Bad: a bootstrap-critical helper uses a tracking-adjacent URL such as `fingerprint.js` even
  though it performs no tracking, so a client filter can collapse the entire module graph.

### 6. Tests Required

- Rust embedded-asset tests assert the canonical route returns 200 with the expected body and the
  removed route returns 404.
- Node render-lifecycle tests import the canonical file and assert the live module asset list
  includes `render-key.js` and contains no `fingerprint.js` URL.
- JavaScript syntax and render-cache behavior tests continue to cover stable serialization and
  unchanged-panel dirty checks.
- When the reported failure is extension-specific, verify in the same user browser profile that
  `Network.loadingFailed` and runtime exceptions are absent and the dashboard renders.

### 7. Wrong vs Correct

#### Wrong

```js
import { panelFingerprint } from './data/fingerprint.js';
```

#### Correct

```js
import { panelFingerprint } from './data/render-key.js';
```

The implementation can retain domain-accurate internal symbol names; the compatibility boundary
is the browser-visible URL that client filters evaluate before module execution.
