# Parse issue payload budget

Interactive dashboard JSON budget is 128 KiB.
`sync_command_center` is already in the interactive snapshot.
This task adds four u64 counters per source and must not add parse-issue samples to that payload.
Samples stay in diagnostics JSON and CLI only.
