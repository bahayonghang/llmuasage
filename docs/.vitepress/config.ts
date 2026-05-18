import { defineConfig } from "vitepress";

const adrItems = [
  { text: "ADR index", link: "/adr/" },
  { text: "0001 — SourceParser trait + registry", link: "/adr/0001-source-registry-and-parser-trait" },
  { text: "0002 — SyncShard as commit protocol", link: "/adr/0002-sync-shard-as-commit-protocol" },
  { text: "0003 — Store façade vs substores", link: "/adr/0003-store-facade-vs-substores" },
  { text: "0004 — Schema version migrations", link: "/adr/0004-schema-version-migration-runner" },
  { text: "0005 — In-memory JobRegistry", link: "/adr/0005-job-registry-in-memory" },
  { text: "0006 — Source file state machine", link: "/adr/0006-source-file-state-machine" },
  { text: "0007 — Error surface", link: "/adr/0007-llmusage-error-surface" },
];

const enSidebar = [
  {
    text: "Guide",
    items: [
      { text: "Overview", link: "/guide/getting-started" },
      { text: "Install and initialize", link: "/guide/install-and-init" },
      { text: "First sync", link: "/guide/first-sync" },
      { text: "First report", link: "/guide/first-report" },
      { text: "Export HTML", link: "/guide/export-html" },
    ],
  },
  { text: "Dashboard", items: [{ text: "Using llmusage serve", link: "/dashboard/" }] },
  {
    text: "Reference",
    items: [
      { text: "CLI commands", link: "/reference/cli" },
      { text: "Library API", link: "/reference/library-api" },
      { text: "Legacy commands page", link: "/commands/" },
    ],
  },
  { text: "Safety", items: [{ text: "Local data and rebuild safety", link: "/safety/" }] },
  {
    text: "Architecture",
    items: [
      { text: "Architecture overview", link: "/architecture/" },
      { text: "PRD archive", link: "/prd/" },
    ],
  },
  { text: "ADR", items: adrItems },
];

const zhSidebar = [
  {
    text: "指南",
    items: [
      { text: "总览", link: "/zh/guide/getting-started" },
      { text: "安装与初始化", link: "/zh/guide/install-and-init" },
      { text: "第一次同步", link: "/zh/guide/first-sync" },
      { text: "第一次报表", link: "/zh/guide/first-report" },
      { text: "导出 HTML", link: "/zh/guide/export-html" },
    ],
  },
  { text: "Dashboard", items: [{ text: "使用 llmusage serve", link: "/zh/dashboard/" }] },
  {
    text: "参考",
    items: [
      { text: "CLI 命令", link: "/zh/reference/cli" },
      { text: "库 API", link: "/zh/reference/library-api" },
      { text: "旧命令页", link: "/zh/commands/" },
    ],
  },
  { text: "安全", items: [{ text: "本地数据与重建安全", link: "/zh/safety/" }] },
  {
    text: "架构",
    items: [
      { text: "架构总览", link: "/zh/architecture/" },
      { text: "PRD 历史档案", link: "/zh/prd/" },
    ],
  },
  { text: "ADR", items: [{ text: "中文说明", link: "/zh/adr/" }, ...adrItems] },
];

export default defineConfig({
  title: "llmusage",
  description: "Local-first AI CLI usage analytics with Rust, SQLite, hooks, and zero upload.",
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    logo: "/logo.svg",
    search: { provider: "local" },
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Dashboard", link: "/dashboard/" },
      { text: "Reference", link: "/reference/cli" },
      { text: "Safety", link: "/safety/" },
      { text: "Architecture", link: "/architecture/" },
      { text: "ADR", link: "/adr/" },
    ],
    sidebar: {
      "/zh/": zhSidebar,
      "/": enSidebar,
    },
    socialLinks: [{ icon: "github", link: "https://github.com/bahayonghang/llmuasage" }],
    footer: {
      message: "Local-first by design.",
      copyright: "Copyright © 2026 llmusage",
    },
  },
  locales: {
    root: {
      label: "English",
      lang: "en",
      themeConfig: {
        nav: [
          { text: "Guide", link: "/guide/getting-started" },
          { text: "Dashboard", link: "/dashboard/" },
          { text: "Reference", link: "/reference/cli" },
          { text: "Safety", link: "/safety/" },
          { text: "Architecture", link: "/architecture/" },
          { text: "ADR", link: "/adr/" },
        ],
        sidebar: { "/": enSidebar },
      },
    },
    zh: {
      label: "简体中文",
      lang: "zh-CN",
      link: "/zh/",
      title: "llmusage",
      description: "本地优先的 AI CLI 用量分析工具，基于 Rust、SQLite 和本地 hook。",
      themeConfig: {
        nav: [
          { text: "指南", link: "/zh/guide/getting-started" },
          { text: "Dashboard", link: "/zh/dashboard/" },
          { text: "参考", link: "/zh/reference/cli" },
          { text: "安全", link: "/zh/safety/" },
          { text: "架构", link: "/zh/architecture/" },
          { text: "ADR", link: "/zh/adr/" },
        ],
        sidebar: { "/zh/": zhSidebar },
        footer: {
          message: "默认本地运行，不上传。",
          copyright: "Copyright © 2026 llmusage",
        },
      },
    },
  },
});
