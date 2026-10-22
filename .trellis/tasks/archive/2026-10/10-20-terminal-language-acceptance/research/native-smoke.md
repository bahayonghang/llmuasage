# Native terminal acceptance

Date: 2026-10-21. Environment: Codex desktop on Windows.

| Surface | English | Chinese | Evidence / limit |
| --- | --- | --- | --- |
| Ordinary-window CLI lang/help/sync | NOT_RUN | NOT_RUN | Native computer APIs are disabled in this session; no ordinary interactive window was controlled. |
| Ordinary-window dash and tui | NOT_RUN | NOT_RUN | No native keyboard/mouse/raw-mode restoration observation is available. |

Headless CLI subprocess captures, local mock-provider tests and TestBackend
rendering are separate PASS evidence in the owning child logs. They are not
native terminal evidence. No screenshot, native success or interactive quota
network call is claimed. The remaining manual smoke should use temporary homes,
synthetic input and local mock endpoints, covering navigation, dialogs, source
selection, sorting, email visibility and terminal restoration in both languages.
