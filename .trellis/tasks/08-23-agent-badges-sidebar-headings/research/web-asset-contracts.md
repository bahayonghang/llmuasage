# Web SVG asset contract extract

## Scope

This note extracts only the repository contracts needed by the Agent Logo task so Trellis context injection does not truncate the full dashboard performance specification.

## Embedded asset manifest

Source: `.trellis/spec/llmusage/backend/web-server-contracts.md:284-356`.

- Every browser-visible file under `src/web/assets/` is registered exactly once in `ASSET_MANIFEST`.
- The fixed Rust array length must equal the actual entry count.
- Live serving and `export html` iterate the same manifest; a Logo cannot exist only in one mode.
- Each Logo declares the correct `image/svg+xml` MIME type.
- Tests compare expected and actual manifest inventories, require unique paths/MIME types, fetch new assets through the live router, and verify representative nested assets beside `index.html` and `snapshot.json` after export.
- `just ci` is required after an inventory change.

## Transfer behavior

Source: `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md:422-489`.

- Embedded asset responses preserve `Cache-Control: no-cache` and a stable content ETag.
- Matching strong or weak `If-None-Match` returns `304` with an empty body.
- gzip/Brotli compression remains available for eligible assets.
- Tests preserve ETag/304, compression, and stable root HTML behavior.

## Browser-visible naming

Source: `.trellis/spec/llmusage/backend/web-server-contracts.md:202-265`.

- Browser-visible asset paths should use truthful, domain-relevant names and avoid misleading telemetry/tracking terminology that content filters may block.
- The proposed `agent-logos/<stable_id>.svg` path truthfully describes product-identity presentation assets and is not bootstrap-critical JavaScript.

## Task consequence

Implementation must update the SVG source files, manifest entries, exact inventory/count tests, live route tests, export tests, ETag/304 coverage, and compression coverage as one atomic asset-pipeline change.
