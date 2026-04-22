import { defineConfig } from "vitepress";

export default defineConfig({
  title: "llmusage",
  description: "Local-first AI CLI usage analytics with Rust, SQLite, hooks, and zero upload.",
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    logo: "/logo.svg",
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Architecture", link: "/architecture/" },
      { text: "Commands", link: "/commands/" },
    ],
    sidebar: {
      "/": [
        {
          text: "Guide",
          items: [
            { text: "Getting Started", link: "/guide/getting-started" },
            { text: "Architecture", link: "/architecture/" },
            { text: "Commands", link: "/commands/" },
          ],
        },
      ],
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
          { text: "Architecture", link: "/architecture/" },
          { text: "Commands", link: "/commands/" },
        ],
        sidebar: {
          "/": [
            {
              text: "Guide",
              items: [
                { text: "Getting Started", link: "/guide/getting-started" },
                { text: "Architecture", link: "/architecture/" },
                { text: "Commands", link: "/commands/" },
              ],
            },
          ],
        },
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
          { text: "架构", link: "/zh/architecture/" },
          { text: "命令", link: "/zh/commands/" },
        ],
        sidebar: {
          "/zh/": [
            {
              text: "指南",
              items: [
                { text: "快速开始", link: "/zh/guide/getting-started" },
                { text: "架构说明", link: "/zh/architecture/" },
                { text: "命令参考", link: "/zh/commands/" },
              ],
            },
          ],
        },
        footer: {
          message: "默认本地运行，不上传。",
          copyright: "Copyright © 2026 llmusage",
        },
      },
    },
  },
});
