# Design: Immutable Self Update

## Resolver

- `UpdateResolver` 返回 `ResolvedUpdate { channel, version/tag, commit_sha, source }`。
- stable resolver 从 release/tag ref 获取 SHA，并验证 ref 与 commit 的对应关系。
- dev resolver 解析 branch 当前 SHA，但结果标记 `mutable_channel=true`。

## Planner

- `UpdatePlanner` 只接受 resolved target，生成 preview 和 process argv。
- stable 使用 `cargo install --git <repo> --rev <sha> --locked --force`。
- confirmation 之后不得再次静默解析另一个 SHA；若需要刷新，必须重新 preview/confirm。

## Test Boundary

- resolver 使用 fake ref provider；executor 使用现有 injected process seam。
- 测试覆盖 moved tag、short/invalid SHA、network error、cancel 和 exact argv。

## Compatibility

- CLI channel 名称和 `--check` 保持不变。
- 输出增加 resolved tag/SHA，不访问 `~/.llmusage`。

