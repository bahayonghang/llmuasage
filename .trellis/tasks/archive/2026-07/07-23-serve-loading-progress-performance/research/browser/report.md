# Scoped Browser Performance Report

## Scope

- Target: `http://127.0.0.1:37421/`
- Focus: initial dashboard loading and the sync command-center placeholder
- Session: fresh isolated Chromium session; no sync button click and no data mutation
- Privacy: live screenshots and HAR were inspected locally, then deleted because they contained real usage data

## Result

The reported 60-second hang did not reproduce. A fresh warm browser load completed without console errors, but dashboard data remained all-or-nothing behind one 1.194-second, ~613 KiB decoded full JSON request. This is a reproducible medium UX/performance issue because the page exposes no bounded timeout or granular progress while that request is unresolved.

## Follow-up Correction

After this warm-path measurement, the user clarified that the affected page remained empty for 30 minutes. A later live check found no `37421` listener or `llmusage` process, and controlled fault injection proved that module/API failure can leave the static sync-center loading copy visible. This report therefore describes only the healthy performance path; it is not the root-cause report for the 30-minute symptom. See `../server-lifecycle.md`.

## Reproduction

1. Open the live local dashboard in a fresh browser session.
2. Inspect the resource timeline before interacting with the page.
3. Observe that scripts/styles complete quickly and one `/api/dashboard?window=day&range=1d` fetch dominates.
4. Observe that dashboard panels render only after that request completes.

## Evidence Summary

- DOMContentLoaded ~148ms; load event ~151ms.
- Full dashboard fetch ~1,194ms, ~628,235 decoded bytes.
- No browser console errors or failed network requests.
- Current test cannot prove the user-reported 60-second duration; it does prove the indefinite-wait UX has no client deadline.
