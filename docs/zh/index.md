---
layout: home

hero:
  name: "llmusage"
  text: "本地优先的 AI CLI 用量分析"
  tagline: "用 hook、SQLite 和 Rust 追踪 Codex、Claude、OpenCode、Google Antigravity，全程不上传。"
  actions:
    - theme: brand
      text: 从指南开始
      link: /zh/guide/getting-started
    - theme: alt
      text: 查看 Dashboard 文档
      link: /zh/dashboard/
    - theme: alt
      text: English Docs
      link: /

features:
  - title: 全本地数据链路
    details: 外部工具只触发本地 hook 或 plugin；解析、聚合、展示都在本机完成。
  - title: SQLite 单一真源
    details: cursor、usage event、30 分钟 bucket、行为事实、source-file 诊断和 run log 都放在本地数据库里。
  - title: 一套查询层复用四种界面
    details: 命令行报表、llmusage dash、llmusage serve 和 export html 共用同一套查询结果。
---

## 按任务选择入口

| 任务 | 入口 |
| --- | --- |
| 安装并初始化本地 hook | [安装与初始化](./guide/install-and-init) |
| 导入本地用量 | [第一次同步](./guide/first-sync) |
| 查看 token 与成本报表 | [第一次报表](./guide/first-report) |
| 查看 Codex 专属调用明细 | [Codex Tracer](./guide/codex-tracer) |
| 使用浏览器 Dashboard | [Dashboard](./dashboard/) |
| 导出静态报告 | [导出 HTML](./guide/export-html) |
| 检查破坏性边界 | [安全说明](./safety/) |
| 查精确参数 | [CLI 参考](./reference/cli) |

## Dashboard 预览

`llmusage serve` 默认在 `127.0.0.1` 启动浏览器 Dashboard；使用 `--public` 才会显式开启远程监听。

![llmusage 本地 Web Dashboard 概览](/screenshots/web-dashboard-overview.png)

<small>截图来自 `llmusage serve` 启动的脱敏本地 fixture，不是真实用户数据。</small>

## 当前产品表面

- 版本：`1.0.1`。
- 来源：Codex、Claude Code、OpenCode、Google Antigravity（source id 为 `antigravity`）。
- 报表命令：`daily`、`monthly`、`session`、`blocks`、`statusline`。
- 本地界面命令：`dash`、`serve`、`export html`。
- 安全/运维命令：`status`、`diagnostics`、`doctor`、`uninstall`。

## 开发命令

```powershell
just install
just build
just docs
just ci
```
