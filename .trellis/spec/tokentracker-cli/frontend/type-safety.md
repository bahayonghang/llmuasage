# TokenTracker Type Safety

## Module Systems

Keep module systems separated:

- Root `src/` is CommonJS.
- `dashboard/` is ESM + strict TypeScript.
- Do not mix imports/exports across these areas without an explicit bridge.

## Token Fields

Use the token field semantics documented in `CLAUDE.md`:

- `input_tokens` is non-cached input only.
- `cached_input_tokens` is cache reads.
- `cache_creation_input_tokens` is cache writes.
- `reasoning_output_tokens` is tracked separately.
- `total_tokens` is the sum of all channels.
- Cost code must use channel fields, not `total_tokens`.

## API And Hook Payloads

Dashboard hooks should normalize API payloads before passing data to components.
Components should not guess whether a row is local, mock, cached, or cloud-backed
from raw response shapes.

## Avoid

- Do not introduce TypeScript laxness in the dashboard to work around API shape
  uncertainty; define the shape at the hook/API boundary.
- Do not infer provider pricing from display labels.
- Do not make raw queue entries a UI component contract.
