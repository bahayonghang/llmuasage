# 统一配额缓存命中来源

## Goal

桌面cache_hit来自subscription实际使用缓存的分支；不通过文件mtime推断。 优先级 P2；状态 planning，等待用户确认后实施。

## Confirmed facts

desktop/src-tauri/src/commands/runtime.rs:61使用mtime；src/subscription/cache.rs:14使用JSON解析与fetched_at；src/subscription/mod.rs:59决定实际cache/live分支，两个判断可能分歧。

## Requirements

- R1：桌面cache_hit来自subscription实际使用缓存的分支；不通过文件mtime推断。
- R2：强制刷新、过期/损坏缓存、有效缓存的返回结果和网络调用与命中标记一致；不改变凭据写入边界。

## Acceptance Criteria

- [x] AC1（R1,R2）：刚写入但fetched_at过期的缓存触发本地fixture endpoint并cache_hit=false；损坏JSON也不误报true。
- [x] AC2（R1,R2）：内部fetched_at新鲜但mtime旧的缓存不触发endpoint且cache_hit=true；bypass_cache=true必须live/false。
- [x] AC3（R2）：原有桌面29项后端测试和64项前端测试继续通过，凭据文件字节不变；CLI/TUI subscription调用者保持同样报告内容。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。test-gates后实施；公开API变化必须在semver基线修复后重新审查。
