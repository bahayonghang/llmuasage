import { getLocale, getShellCopy, onLocaleChange } from './copy.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：把 i18n key 应用到 DOM
 * ========================================================================
 * 目标：
 * 1) [data-i18n="key"]        → textContent 替换
 * 2) [data-i18n-html="key"]   → innerHTML 替换（用于 hero 标题里的 <span> 高亮）
 * 3) [data-i18n-attr="attr=key,attr=key"] → 单个或多个属性赋值（aria-label / title）
 * 4) <html data-locale> 镜像当前 locale，给 CSS 提供选择器钩子
 * 5) <html data-i18n-title="key"> → document.title
 */
export function applyDomI18n(root = document) {
  logger.info('开始应用 DOM i18n');

  // 1.1 同步 <html data-locale>，CSS 选择器靠它
  document.documentElement?.setAttribute('data-locale', getLocale());

  // 1.2 文本节点
  root.querySelectorAll('[data-i18n]').forEach((el) => {
    const key = el.getAttribute('data-i18n');
    if (!key) return;
    el.textContent = getShellCopy(key);
  });

  // 1.3 富文本节点（HTML）
  root.querySelectorAll('[data-i18n-html]').forEach((el) => {
    const key = el.getAttribute('data-i18n-html');
    if (!key) return;
    el.innerHTML = getShellCopy(key);
  });

  // 1.4 属性
  root.querySelectorAll('[data-i18n-attr]').forEach((el) => {
    const spec = el.getAttribute('data-i18n-attr');
    if (!spec) return;
    spec.split(',').forEach((pair) => {
      const [attr, key] = pair.split('=').map((part) => part.trim());
      if (attr && key) {
        el.setAttribute(attr, getShellCopy(key));
      }
    });
  });

  // 1.5 文档标题
  const titleKey = document.documentElement?.getAttribute?.('data-i18n-title');
  if (titleKey) {
    document.title = getShellCopy(titleKey);
  }

  logger.info('完成 DOM i18n 应用');
}

/*
 * ========================================================================
 * 步骤2：locale 变更时自动重新应用
 * ========================================================================
 * 目标：
 * 1) toggle 处只需调 setLocale，不必手动同步 DOM
 * 2) 同时通知调用方做组件级重渲染（外部订阅 onLocaleChange）
 */
export function bindI18nDomSync() {
  logger.info('开始绑定 i18n DOM 同步');

  onLocaleChange(() => {
    applyDomI18n(document);
  });

  logger.info('完成 i18n DOM 同步绑定');
}
