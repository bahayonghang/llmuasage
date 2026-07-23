/*
 * ========================================================================
 * 面板渲染键（纯模块，无 DOM/window 依赖，可独立进 node 测试）
 * ========================================================================
 * 目标：
 * 1) 渲染前比对"本面板数据指纹"，未变则跳过 DOM 写入（面板级 dirty-check）
 * 2) 指纹 = locale + 面板消费的 rawData 子集 + 面板级额外状态 的稳定序列化
 * 3) 每查询必变的服务端字段在指纹前剔除，避免 dirty-check 永不命中
 */

/*
 * 易变字段剔除清单（均为服务端 now_utc()，每次查询必变：
 * OverviewPayload 见 src/query/mod.rs overview 构造，
 * SyncCommandCenterPayload 见同文件 sync_command_center 构造）。
 * 相对时间文案（如"x 分钟前"）由渲染层从时间戳派生，不在服务端数据字段内，
 * 因此无需进一步剔除。
 */
const VOLATILE_FIELD_PATHS = Object.freeze([
  Object.freeze(['overview', 'generated_at']),
  Object.freeze(['sync_command_center', 'generated_at']),
]);

/*
 * 各渲染面板消费的 rawData 顶层字段子集。
 * 原则：宁可多列（数据没变但多渲染一次，无害）不可漏列（数据变了却不渲染）。
 */
const PANEL_DATA_KEYS = Object.freeze({
  syncCommandCenter: Object.freeze(['sync_command_center']),
  hero: Object.freeze(['overview', 'health', 'diagnostics', 'models', 'sources', 'costs']),
  trends: Object.freeze(['trends', 'sources', 'overview']),
  models: Object.freeze(['models']),
  sources: Object.freeze(['sources', 'overview']),
  projects: Object.freeze(['projects']),
  costs: Object.freeze(['costs', 'models', 'health']),
  insights: Object.freeze(['overview', 'models', 'projects', 'costs', 'sources', 'diagnostics', 'health']),
  activity: Object.freeze(['activity']),
  tools: Object.freeze(['tools']),
  optimize: Object.freeze(['optimize']),
  compare: Object.freeze(['compare']),
  explorer: Object.freeze(['explorer']),
});

// key 排序的稳定序列化：相同语义内容必得相同字符串，与对象键序无关。
export function stableSerialize(value) {
  if (value === null || typeof value !== 'object') {
    const serialized = JSON.stringify(value);
    return serialized === undefined ? 'null' : serialized;
  }
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableSerialize(item)).join(',')}]`;
  }
  const keys = Object.keys(value)
    .filter((key) => value[key] !== undefined && typeof value[key] !== 'function')
    .sort();
  return `{${keys.map((key) => `${JSON.stringify(key)}:${stableSerialize(value[key])}`).join(',')}}`;
}

// 返回剔除易变字段后的副本；不原地修改入参（rawData 约定不可变替换式更新）。
export function stripVolatileFields(rawData) {
  if (!rawData || typeof rawData !== 'object') {
    return rawData;
  }
  let stripped = rawData;
  for (const [section, field] of VOLATILE_FIELD_PATHS) {
    const sectionValue = stripped[section];
    if (sectionValue && typeof sectionValue === 'object' && field in sectionValue) {
      if (stripped === rawData) {
        stripped = { ...rawData };
      }
      const nextSection = { ...sectionValue };
      delete nextSection[field];
      stripped[section] = nextSection;
    }
  }
  return stripped;
}

/*
 * 面板级指纹。options：
 * - locale：文案语言。locale 变化令指纹自然失效，触发文案重渲，无需特判。
 * - extra：面板级额外状态（expanded、job overlay、secondary refreshing 标记等）。
 */
export function panelFingerprint(panel, rawData, options = {}) {
  const keys = PANEL_DATA_KEYS[panel];
  if (!keys) {
    return null;
  }
  const stripped = stripVolatileFields(rawData || {});
  const subset = {};
  for (const key of keys) {
    subset[key] = stripped[key];
  }
  const locale = options.locale || '';
  const extra = 'extra' in options ? stableSerialize(options.extra) : '';
  return `${panel}|${locale}|${stableSerialize(subset)}|${extra}`;
}

/*
 * 整体数据指纹：自动刷新 / sync 完成后判断"数据是否真的变了"。
 * _meta 是客户端渲染态元数据（secondary_refreshing 等），不属于数据指纹。
 */
export function dashboardFingerprint(rawData) {
  const stripped = stripVolatileFields(rawData || {});
  const { _meta, ...data } = stripped;
  return stableSerialize(data);
}
