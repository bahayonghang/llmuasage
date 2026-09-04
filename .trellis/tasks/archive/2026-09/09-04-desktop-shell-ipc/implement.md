# Desktop shell and IPC — Implement

## Checklist

1. 脚手架 `desktop/` Tauri 2。
   - 文件：`desktop/package.json`、`desktop/src-tauri/tauri.conf.json`、`desktop/src-tauri/Cargo.toml`、`desktop/src-tauri/src/main.rs`
   - 符号：identifier `com.bahayonghang.llmusage`；path dep `../..`
   - 验证：`cargo metadata --manifest-path desktop/src-tauri/Cargo.toml`

2. 错误映射。
   - 文件：`desktop/src-tauri/src/error.rs`
   - 符号：`map_llmusage_error`、`code` 集合
   - 验证：单元测试每个 `LlmusageError` 变体 → code

3. DTO 与受检转换。
   - 文件：`desktop/src-tauri/src/dto.rs`
   - 符号：`FilterDto`、`convert_filter`、`convert_explorer`、`convert_logs`、`convert_sync`
   - 验证：`cargo test convert_filter convert_sync convert_logs --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1`

4. 启动与 `AppState`。
   - 文件：`desktop/src-tauri/src/state.rs`
   - 符号：`startup`、`AppContext::with_cli_home`、`repair_legacy_token_accounting`
   - 验证：空 root bootstrap 测试；`SchemaTooNew` 夹具

5. 查询 supervisor 与 `interrupt_handle` 窄补丁。
   - 文件：`src/query/mod.rs`、`desktop/src-tauri/src/supervisor.rs`
   - 符号：`Dashboard::interrupt_handle`（`pub`）、`DesktopQuerySupervisor`、`cancel_queries`
   - 验证：AC6 取消测试；`python scripts/ci-rust.py`（因改根 crate）

6. 注册 command 表（含 `start_sync` rebuild=false、`fetch_quota` 可注入）。
   - 文件：`desktop/src-tauri/src/commands/*.rs`、`main.rs`
   - 符号：父 design command 表
   - 验证：snapshot 形状、`lock_busy`、AC7、AC9

7. 单实例插件。
   - 文件：`tauri.conf.json`、`main.rs`
   - 符号：`tauri-plugin-single-instance`
   - 验证：配置含插件；焦点人工留给父门禁

8. 最小前端按钮调 `runtime_info`。
   - 文件：`desktop/src/main.tsx` 或占位 `index.html`
   - 验证：`tauri dev` 能打印 `root_dir`（开发者本机临时 root 或默认 discover）

## Validate

```
cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1
```

改了 `src/query/mod.rs` 时：

```
python scripts/ci-rust.py
```

未改其它根 `src/` / CI 文件时不要为这个子任务跑满 `just ci`。

## Rollback

删除 `desktop/`。还原 `Dashboard::interrupt_handle` 可见性。
