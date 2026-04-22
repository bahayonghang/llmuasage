# 架构说明

## 运行时目录

运行时目录固定在 `~/.llmusage/`：

- `llmusage.db`：保存 cursor、event、bucket、项目元数据、集成状态、trigger 状态和运行日志
- `bin/llmusage-hook.cmd` 与 `bin/llmusage-hook.sh`：外部工具调用的本地包装器
- `exports/`：静态 HTML 报告输出目录
- `backups/`：卸载时回滚配置用的备份

## 数据流

1. 外部工具触发本地 hook 或 plugin
2. `llmusage hook-run` 先记录 trigger，再尝试拿全局锁
3. worker 顺序消费 Codex、Claude、OpenCode 三类本地真源
4. 新事件写入 `usage_event`
5. 30 分钟 UTC 聚合写入 `usage_bucket_30m`
6. 本地 Web UI、TUI、静态导出都从同一个 SQLite 读数据

## 本地优先边界

- 不生成 device token
- 不走登录流程
- 不做上传队列
- 不访问远端 API
- 不做 GitHub 在线公开性探测

项目展示名优先使用本地 git remote；本地路径默认只存 hash。
