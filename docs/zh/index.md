---
layout: home

hero:
  name: "llmusage"
  text: "本地优先的 AI CLI 用量分析"
  tagline: "用 hook、SQLite 和 Rust 追踪 Codex、Claude、OpenCode，全程不上传。"
  actions:
    - theme: brand
      text: 快速开始
      link: /zh/guide/getting-started
    - theme: alt
      text: English Docs
      link: /

features:
  - title: 全本地数据链路
    details: 外部工具只触发本地 hook 或 plugin，解析、聚合、展示都在本机完成。
  - title: SQLite 单一真源
    details: cursor、usage event、30 分钟 bucket、集成状态和运行日志都放在一个本地数据库里。
  - title: 一套查询层复用三种界面
    details: 浏览器分析页、TUI 和静态 HTML 导出共用同一套查询结果。
---

## v1 功能

- Codex `notify`
- Claude `Stop` / `SessionEnd`
- OpenCode `session.updated`
- 本地 Web UI
- TUI 运维面板
- 静态 HTML 导出

## Web 分析页预览

下面这张图就是 `llmusage serve` 暴露出来的本地浏览器分析页。

![llmusage 本地 web 分析页概览](/screenshots/web-dashboard-overview.png)

## 开发命令

```powershell
just install
just build
just docs
just ci
```
