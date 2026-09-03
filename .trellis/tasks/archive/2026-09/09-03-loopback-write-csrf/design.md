# Design: loopback write CSRF

## Choice

Origin/Host allowlist, not a new session token.

Allowed Origin/Host:

- `http://127.0.0.1:<bound-port>`
- `http://localhost:<bound-port>`
- `http://[::1]:<bound-port>` if IPv6 loopback is ever bound (today IPv4 only)

Rules:

1. Peer must still be loopback.
2. If `Origin` present, it must match the allowlist.
3. If `Origin` absent, `Host` must be `127.0.0.1:<port>` or `localhost:<port>` (browsers send Host; CSRF from a public site typically sends their Origin).
4. Non-browser clients (curl) without Origin succeed only with loopback Host.

Drop `rebuild` from HTTP `SyncRequestInput` unless the live UI posts it. Check `src/web/assets/render/sync-command-center.js` before removing.

Do not change `WriteExposure::PublicReadOnly` route inventory.

## Tests

Real TCP, matching existing public/loopback tests in `web/mod.rs`.
