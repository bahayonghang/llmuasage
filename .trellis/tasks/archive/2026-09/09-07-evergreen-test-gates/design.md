# 补齐桌面与看板测试门禁：设计

## Mechanism and tradeoffs

增加一个很小的 scripts/ci-js.mjs：用 Node fs 枚举 scripts/tests/*.test.mjs，执行现有两项 node --check，再 spawn 同一 Node 的 --test，并逐级传递退出码。它是测试运行入口，不引入自定义规则引擎。just ci 和 Actions只调用此入口。

在现有justfile中增加 desktop-check（npm test、现有本地 tsc、npm build、cargo test --locked --manifest-path ...）；desktop-test委派该统一检查或按明确命名保留测试子集，文档不可再称它包含类型/构建。CI新增Windows桌面job，先 npm ci --prefix desktop，再同等命令，Node使用已兼容Vite7的22版本。根Rust三平台矩阵保持不变。保留 CI gate 的名字，仅补needs。

从 just ci移走cargo update；version-sync现有显式更新仍保留。不把MSRV/security/semver外部基线检查伪称成本地just ci的隐含内容；主说明给出完整检查矩阵。

## File ownership

- `justfile`
- `scripts/ci-js.mjs`
- `.github/workflows/ci.yml`
- `desktop/package.json`
- `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`

## Tool and model assignment

Codex/Claude Code 强模型审查漏测和根/桌面工作区边界；定稿后便宜模型适合 Node执行入口、YAML接线、命令去重。OMP/Kimi可在已确认模型与技能加载后执行同样有限文件任务。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
