const logger = window.console;

export const THEMES = Object.freeze(['light', 'dark']);
export const DEFAULT_THEME = 'light';
export const THEME_STORAGE_KEY = 'llmusage:theme';

const themeListeners = new Set();
let currentTheme = readStoredTheme();

function readStoredTheme() {
  // 单一事实源：初始主题由 shell.rs 内联脚本解析（stored ?? prefers-color-scheme ?? light）
  // 并写入 <html data-theme>；此处优先采信该属性，避免与内联脚本重复解析而分叉。
  const domTheme = document.documentElement?.getAttribute('data-theme');
  if (THEMES.includes(domTheme)) return domTheme;
  try {
    const stored = window.localStorage?.getItem(THEME_STORAGE_KEY);
    if (THEMES.includes(stored)) return stored;
  } catch (_err) {
    /* 隐私模式下读失败时回退系统偏好 */
  }
  return resolvePreferredTheme();
}

function resolvePreferredTheme() {
  try {
    if (window.matchMedia?.('(prefers-color-scheme: dark)').matches) {
      return 'dark';
    }
  } catch (_err) {
    /* matchMedia 不可用时回退默认 */
  }
  return DEFAULT_THEME;
}

/*
 * ========================================================================
 * 步骤1：读取 / 写入主题状态
 * ========================================================================
 * 目标：
 * 1) <html data-theme="light|dark"> 是唯一权威，CSS 变量靠它分流
 * 2) 持久化到 localStorage，刷新后保留
 * 3) 设置过的主题复用旧值时跳过广播
 */
export function getTheme() {
  return currentTheme;
}

export function setTheme(theme) {
  logger.info('开始切换主题');

  // 1.1 标准化输入
  const next = THEMES.includes(theme) ? theme : DEFAULT_THEME;
  if (next === currentTheme) {
    applyThemeAttribute(next);
    logger.info('主题未变化，跳过');
    return next;
  }

  // 1.2 更新状态
  currentTheme = next;
  applyThemeAttribute(next);

  try {
    window.localStorage?.setItem(THEME_STORAGE_KEY, next);
  } catch (_err) {
    /* 隐私模式下忽略写失败 */
  }

  // 1.3 通知订阅者
  for (const cb of themeListeners) {
    try {
      cb(next);
    } catch (err) {
      logger.error('主题监听器抛错', err);
    }
  }

  logger.info('完成主题切换');
  return next;
}

export function toggleTheme() {
  return setTheme(currentTheme === 'dark' ? 'light' : 'dark');
}

export function onThemeChange(callback) {
  if (typeof callback !== 'function') return () => {};
  themeListeners.add(callback);
  return () => themeListeners.delete(callback);
}

/*
 * ========================================================================
 * 步骤2：把当前主题立即写入文档根
 * ========================================================================
 */
export function initTheme() {
  applyThemeAttribute(currentTheme);
  return currentTheme;
}

function applyThemeAttribute(theme) {
  const root = document.documentElement;
  if (root) {
    root.setAttribute('data-theme', theme);
  }
}
