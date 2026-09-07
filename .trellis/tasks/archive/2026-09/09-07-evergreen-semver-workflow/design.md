# 修复 Semver 工作流与同源基线：设计

## Mechanism and tradeoffs

在现有 Architecture checks 内修正命令，checkout 获取已存在 release tag 的历史（fetch-depth: 0）。显式 v1.2.0 作为当前已验证 release 基线；这是发布基线，需要随正式 release 更新，不能每次自动改成 HEAD。取消 main-only step 条件，让 PR 和手动验证覆盖同一真实命令。继续使用现有已安装步骤，不引入新的 action/工作流框架。工具本机尚未安装；实施若安装只使用项目隔离工具目录，不改变用户全局环境，所用版本及 Rust 兼容性须记录。

当前业务版本 1.3.0 与基线1.2.0的 API兼容性未验证；修正检查后可能揭露真实 API 差异。这是明确的后续判定点，不以本次全部单测绿色替代 semver 结果。允许停止于报告真实新差异，待单独批准版本或 API 改动。删除错误 documentation 字段比编造线上API文档链接更小。

## File ownership

- `.github/workflows/ci.yml`
- `Cargo.toml`
- `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`

## Tool and model assignment

Codex/Claude Code 强模型判断本仓库基线与 release 含义；便宜模型可按定稿修改 YAML 和元数据。Grok Build 可作独立命令参数审查。强模型必须检查真实 semver 日志。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
