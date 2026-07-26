# Design: Honest MSRV And Validation Baseline

## MSRV Selection

- 用 clean target/cache 分别运行 1.85、1.88、1.89 的 locked all-features check。
- 选择第一个完整通过的版本，并同步 `rust-version`、MSRV workflow label/toolchain 与相关文档。
- 依赖自身 MSRV 与本 crate 源码 MSRV 都必须验证。

## Subprocess Harness

- 记录 `CARGO_BIN_EXE_llmusage` 的 exact path、存在性、工作目录和 spawn error context。
- 对比同文件其他 subprocess tests，统一 helper，避免局部 path canonicalization/环境覆盖。
- Windows path 处理使用 `PathBuf`/argv，不拼接 shell command。

## Gate Integrity

- CI 的 MSRV、fmt、clippy、tests 使用 locked dependencies。
- 失败必须保留原始 exit/status context，不能将 environment uncertainty 记为 pass。
