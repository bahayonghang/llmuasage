# serve 首屏性能基线（2026-07-23）

## Evidence Boundary

- 用户补充：页面等待 30 分钟仍没有数据。后续检查发现原 `127.0.0.1:37421` 已无 listener/进程，所有根页面与 API 请求均 connection refused；因此 30 分钟现象不能归因于一个仍在运行的慢查询。原进程退出原因因缺少退出日志/exit code 仍未证明，详见 `research/server-lifecycle.md`。
- 初始性能测量实例：`C:\Users\lyh\.cargo\bin\llmusage.exe serve`，版本 `1.0.1`，测量时监听 `127.0.0.1:37421`；用户补充 30 分钟症状后的复查中，该 listener/process 已不存在。
- 已安装二进制与 `target\release\llmusage.exe` 的 SHA-256 相同：`46C77545...F63C7B04`。当前用户未提交的 `Cargo.toml` 仅把版本改为 1.0.2，未纳入本任务。
- 本地数据库约 1,160,073,216 bytes；只记录规模与汇总计数，不保存 API body、HAR、路径明细或事件内容。

## HTTP Measurements

| Request | Wall time | Bytes | Notes |
| --- | ---: | ---: | --- |
| `/` | 0.0009s | 28,289 | HTML/asset server正常 |
| `/api/dashboard?scope=interactive&range=1d&window=day` | 0.106s | 14,303 | 当前服务首次抽样 |
| `/api/dashboard` | 1.088s | 693,701 | curl full 抽样 |
| browser `/api/dashboard?window=day&range=1d` | 1.194s | 628,235 decoded | 首次渲染前唯一阻塞 fetch |
| temporary cold server first full | 0.917s | 639,316 | 新进程 `37422`，完成后已停止 |
| same cold server post-full interactive | 0.006s | 14,303 | 证明 interactive 投影显著更轻 |
| `/api/home_overview` | 2.948s | 36,017 | 慢，但不在当前 serve 首屏调用链 |
| `/api/health` | 0.013s | 612,057 | full 体积主要来自 2,807 cursor rows |

## Representative Secondary Reads

| Endpoint | Isolated wall | Full composition result |
| --- | ---: | --- |
| activity | 0.112s | degraded: 1s timeout |
| tools | 0.852s | degraded: 1s timeout |
| optimize | 0.596s | degraded: 1s timeout |
| compare | 0.360s | degraded: 1s timeout |
| explorer | 0.005s | 6 rows |

The same behavior queries fit individually but time out when full composition starts them together under the four-permit dashboard semaphore. This matches the prior representative-copy evidence in the archived S3 task: full was intentionally kept parallel and degraded rather than made sequential for 10+ seconds.

## Browser Findings

- Navigation DOMContentLoaded: ~148ms; document load: ~151ms.
- Static JS/CSS requests completed in roughly 1-6ms each; no failed request or console exception.
- The app did not render dashboard data until the 1.194s full API request completed.
- The full page issued 27 requests, but only the dashboard JSON request dominated. Bundle/font/image work is not the bottleneck.
- Live browser artifacts were deleted after inspection because HAR and screenshots contained real local usage/model data.

## Root Cause Ranking

1. Confirmed for the 30-minute symptom: the corresponding local listener/process no longer existed, while the browser retained the static shell.
2. Confirmed: module bootstrap failure cannot reach `main()` or `renderBootstrapError`, so the static sync-center loading copy has no terminal transition.
3. Confirmed: even when JS catches total API failure, `renderBootstrapError` does not replace the sync-center placeholder; dashboard failure additionally fans out to 13 legacy endpoints.
4. Confirmed performance amplifier: live initial load still uses the all-or-nothing full endpoint; full serializes the complete health cursor list and is roughly 45x the interactive payload.
5. Confirmed performance amplifier: full secondary concurrency increases SQLite contention and returns four degraded sections on the representative library.
6. Unknown: the exact trigger that ended the original process. Current run/log surfaces record bind success, not session exit.

## Recommended Fix Boundary

First supervise the server lifecycle and add a module-independent bootstrap terminal state. Then use interactive for the live first paint, load secondary sections through the existing concurrency-2/generation-aware path, and add explicit slow/deadline/retry states that replace the static placeholder on every failure branch. Preserve full API/static snapshot compatibility and leave unrelated `home_overview` or schema work out unless new profiling places it on the actual route.
