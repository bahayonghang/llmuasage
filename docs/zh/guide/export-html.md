# 导出 HTML

需要可携带的离线 Dashboard 快照时，使用 `export html`。

## 导出到目录

```powershell
llmusage export html --out .\llmusage-report
```

如果省略 `--out`，llmusage 会写入运行时导出目录。

## 输出文件

导出目录包含：

- `index.html`
- `snapshot.json`
- `assets/*`

该 bundle 复用 `llmusage serve` 的 Dashboard shell，但从 `snapshot.json` 加载数据，而不是访问 live HTTP endpoints。

## 快照行为

静态导出保留导出时的筛选和数据。实时 sync job、自动刷新等 live-only 控件会被禁用并显示说明。

## 推荐流程

```powershell
llmusage sync
llmusage export html --out .\llmusage-report
```

想让导出包含最新本地记录时，先运行一次 sync。
