# Product

## Register

product

## Users

Developers and power users of AI coding CLIs (Codex, Claude Code, OpenCode, Google Antigravity) who run agents daily and want to understand their own token usage, cost, and working patterns. They work locally, often on metered or rate-limited plans, and are privacy-conscious: they will not hand transcripts or usage to a hosted service. They reach for llmusage on the command line, or open the local browser dashboard (`llmusage serve`), to answer questions like "where did my tokens go this week," "which model or project costs the most," and "am I retrying or one-shotting." Most are engineers comfortable with SQLite, hooks, and the terminal.

## Product Purpose

llmusage turns scattered local AI-CLI artifacts into a single local SQLite database and renders it as reports, a terminal dashboard, a browser dashboard, and offline HTML exports, with no upload, login, or telemetry. It exists because usage and cost insight for AI coding tools otherwise means trusting a vendor with your transcripts; llmusage keeps all of that on your machine. Success: a developer answers a real usage/cost/behavior question in seconds, trusts the numbers (or understands exactly why a number is an estimate), and never wonders whether data left their laptop.

## Brand Personality

Local-first, honest, and quietly technical. Three words: trustworthy, precise, calm. The voice says what the tool literally does and is forthright about limits: costs are "estimates that differ from your bill," diagnostic signals "suggest a next step, not a verdict." No hype, no urgency, no vanity metrics. It should feel like a well-made instrument: legible at a glance, dense only where density earns its place, never performing certainty it doesn't have.

## Anti-references

- Hype-y SaaS dashboards: gradient hero-metric templates, "supercharge your workflow" copy, vanity-metric theater.
- Dense ops cockpits (Datadog / Grafana style): neon-on-black walls of widgets that put density ahead of legibility.
- Generic admin templates: Bootstrap-y identical card grids, stock dashboard scaffolding with no point of view.
- Consumer fintech apps: playful, gamified spending-tracker aesthetics. This is a precise developer tool, not a money app.

## Design Principles

1. Local-first, visibly. Make "this never leaves your machine" legible: environment chips, the 127.0.0.1 endpoint, offline-capable snapshots. Privacy is the product; show it.
2. Honest numbers. Estimates are labeled as estimates; low-sample and unsupported sources degrade explicitly rather than faking precision. Never imply more certainty than the data supports.
3. Density that earns its place. Show real data richly, but every panel must answer a question a user actually asks. No widget for the sake of a grid.
4. Glanceable first. A user should find the one number they came for before reading anything else. Hierarchy does the work; decoration doesn't.
5. Restraint is the brand. The calm, warm, low-chrome identity is a deliberate counter to telemetry-console and hype-SaaS conventions. When in doubt, remove.

## Accessibility & Inclusion

Target WCAG 2.1 AA. Body text >=4.5:1 against its background in both light and dark themes; audit the warm-neutral surfaces specifically, since tinted near-whites are the common failure. Full keyboard navigation for nav, filters, and the Cost Explorer controls, with visible focus states. Every animation needs a `prefers-reduced-motion` alternative. Preserve existing ARIA usage (roles, `aria-pressed`, `aria-live` regions) and the bilingual EN/ZH surface. Don't rely on color alone to distinguish chart series.
