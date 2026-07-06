# vibeusage Component Guidelines

## Copy Registry

All visible dashboard text must come from `dashboard/src/content/copy.csv`.
Components should use the shared copy helper instead of hardcoded labels.

Reference files:

- `ref/vibeusage/AGENTS.md`
- `ref/vibeusage/dashboard/src/content/copy.csv`
- `ref/vibeusage/dashboard/src/lib/copy.ts`

## Dashboard Components

The active dashboard uses Matrix UI components under
`dashboard/src/ui/matrix-a/components/` and page orchestration in
`dashboard/src/pages/DashboardPage.jsx`.

Local component rules:

- Keep visual components focused on rendering normalized props.
- Put data fetch, cache, and provenance decisions in hooks or API helpers.
- Use backend-provided `display_model` for presentation.
- Keep `model_id` as the canonical key for filtering, pricing, and aggregation.

## Client Visibility

When adding an AI source, update dashboard visibility surfaces:

- `ClientLogos.jsx` registration for the source.
- landing/dashboard copy entries in `copy.csv`.
- relevant tests and source checklist items from `AGENTS.md`.

## Avoid

- Do not hardcode UI text in JSX.
- Do not use display labels as model keys.
- Do not let components relabel cached or snapshot-backed data as mock data.
