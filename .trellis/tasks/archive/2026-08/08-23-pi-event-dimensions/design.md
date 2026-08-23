# 设计：Pi 事件的 provider 与项目维度

## 边界

改 `src/parsers/pi.rs` 的事件构造、`src/store/sync_writer.rs` 的标签填充条件，
以及会话级元数据的读取顺序。不改 schema，不改 `event_key` 组成。

## 决定 1：provider 填充的优先级

`commit_shard` 的填充条件从「无条件覆盖」改为「仅填空」：

```rust
if let Some(index) = self.provider_index.as_ref() {
    for event in &mut shard.events {
        if event.provider_label.is_empty() {
            event.provider_label = index.label_for(event.source, &event.event_at);
        }
    }
}
```

这条改动符合 ADR 0010 把空串定义为「未归属哨兵」的原意：CCR 时间线负责填补未归属，
不负责推翻源已经给出的事实。影响面是所有源，其中 `deepseek_harness` 由缺陷状态恢复
为设计状态，`codex` / `claude` 行为不变（它们的解析器写空串）。

被否方案：给 `ProviderIndex::label_for` 返回 `Option<String>`，让 `None` 表示
「该源没有时间线」。否决理由：改动扩散到 ADR 0010 定义的公开语义与其单测，
收益与「仅填空」相同。

## 决定 2：项目维度的解析顺序

顺序：

1. 从文件头部读会话头（`{"type":"session", ...}`）的 `cwd`。命中则用
   `ProjectResolver::resolve(cwd)`，拿到 git 仓库根、远端引用与哈希
   （与 `src/parsers/claude.rs` 一致）。本机 28/28 文件都有 `cwd`，所以这是主路径。
2. `cwd` 不可用时，取 **`root` 之下第一段目录名**（不是文件父目录名）解码为
   workspace 字符串，构造 `ProjectInfo`：标签取最后一段路径，`project_ref` 为 `None`，
   三个哈希都用 workspace 哈希（形式同 `src/parsers/grok.rs:1029`）。
3. 两者都不可用时保持 `project: None`。

第 2 步必须按 root 相对路径的第一段推导，因为真源有两层布局：

```text
<root>/--D--Documents-Code-CLI-llmusage--/agent_<session>.jsonl
<root>/--D--Documents-Code-Github-ccr--/2026-08-22T16-26-20-289Z_<uuid>/DiffJudge.jsonl
```

取父目录名会让第二种形态得到 `2026-08-22T16-26-20-289Z_<uuid>`，把同一个项目拆成
多个按运行时间命名的伪项目。ccusage 的两个函数正是这个语义
（`extract_project` 取 `sessions` 之后一段；`extract_store_project` 先 `strip_prefix(root)`
再取第一段），实现时以 root 相对化为准。

编码目录名的形态需要以本机样本确认：`--D--Documents-Code-CLI-llmusage--` 不是 percent
编码，而是把分隔符替换为 `-` 并在两端补 `--`。因此不能复用 `grok.rs` 的
`percent_decode_lossy`，需要单独的解码函数并按本机样本写单测。路径分隔符信息在这种
编码里不可逆（盘符 `D:` 与目录分隔都成了 `-`），所以回落分支只保证标签可读、
同项目一致，不保证还原原始路径——这也是把会话头 `cwd` 放在第一位的原因。

## 决定 3：会话元数据的读取与增量游标的关系

当前 `parse_session_file` 从 `start_offset` 开始读，增量续读时看不到文件开头的会话头。
方案：在 `build_session` 之外增加一次「文件头部有界读」，只读前若干行寻找
`type == "session"`，命中即停。理由与代价：

- 每个文件每次 sync 多一次小体积读，成本可接受（本机 28 个文件）。
- 不改游标语义，不影响 DATA-001 的「部分尾行不推进游标」契约。
- 真源会在 `session` 之前写 `title` 记录（本机 28 个文件里 `title` 与 `session`
  各 28/9 条分布，tokscale 的 `PRE_SESSION_METADATA_TYPES` 说明同一现象），
  因此头部扫描要跳过非 `session` 记录而不是只看第一行。

会话头还带 `id`（uuid）。命名子会话的文件名没有 `_`，现有 `extract_session_id`
会把整个文件名当 session id（例如 `DiffJudge`）。既然已经读到会话头，
`session_id` 优先用会话头的 `id`，文件名派生只作为回落；`session_label` 保留文件名
派生值，便于人读。该改动只影响 `session_id` 的取值，不参与 `event_key`（R2.5）。

`ProjectResolver` 带缓存，按 `cwd` 去重，所以同一项目下多个会话文件只解析一次。

## 决定 4：provider 值原样保留

`message.provider` 的取值是路由标识（`openai-codex`、`xai-oauth`、`openrouter`、
`deepseek`），不做归一或改写。理由：这些值区分的是计费主体（oauth 订阅 vs API key），
归一会丢掉 `08-23-pi-source-cost` 需要的信息——本机数据里 xai-oauth 与 openrouter
的成本恒为 0，openai-codex 与 deepseek 的成本非 0。

## 兼容性

- 无 schema 变化。既有事件不会被增量 sync 重写（`INSERT OR IGNORE`）。
- 历史回填按父任务 R8 用 `sync --rebuild --source omp`，本子任务的所有本机验收都在
  该命令之后执行。
- 回滚：纯代码回退，数据侧无结构变化；已回填的维度需再次 rebuild 才能回到空值。
