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

该导出包复用 `llmusage serve` 的看板外壳，但从 `snapshot.json` 加载数据，而不是访问实时 HTTP 接口。`snapshot.json` 包含固定看板区块和默认用量分析数据。

## 快照行为

静态导出保留导出时的筛选和数据。实时 sync job、自动刷新和自定义 Explorer 重新查询等 live-only 控件会被禁用并显示说明。

## 推荐流程

```powershell
llmusage sync
llmusage export html --out .\llmusage-report
```

想让导出包含最新本地记录时，先运行一次 sync。
