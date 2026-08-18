/*
 * Intl.NumberFormat 构造昂贵且实例不可变：按 locale+options 做模块级缓存复用。
 * 调用点固定为 'en-US' 的少量组合，缓存条目数有界。
 */
const numberFormatterCache = new Map();
let numberFormatterConstructions = 0;

function numberFormatter(locale, options) {
  const key = `${locale}|${JSON.stringify(options || null)}`;
  let formatter = numberFormatterCache.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat(locale, options);
    numberFormatterConstructions += 1;
    numberFormatterCache.set(key, formatter);
  }
  return formatter;
}

// 插桩：构造次数应为有界常数，不随格式化调用次数增长（供性能回归测试断言）。
export function numberFormatterStats() {
  return {
    constructed: numberFormatterConstructions,
    cached: numberFormatterCache.size,
  };
}

export function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

export function formatNumber(value) {
  return numberFormatter('en-US').format(Number(value || 0));
}

const COMPACT_UNITS = [
  { value: 1_000_000_000_000, suffix: 'T' },
  { value: 1_000_000_000, suffix: 'B' },
  { value: 1_000_000, suffix: 'M' },
  { value: 1_000, suffix: 'K' },
];

function formatCompactNumber(value) {
  const amount = Number(value || 0);
  if (!Number.isFinite(amount)) {
    return '0';
  }

  const sign = amount < 0 ? '-' : '';
  const abs = Math.abs(amount);
  let unitIndex = COMPACT_UNITS.findIndex((candidate) => abs >= candidate.value);
  if (unitIndex < 0) {
    return `${sign}${numberFormatter('en-US').format(abs)}`;
  }

  let unit = COMPACT_UNITS[unitIndex];
  let scaled = abs / unit.value;
  const maximumFractionDigits = 1;
  if (Number(scaled.toFixed(maximumFractionDigits)) >= 1000 && unitIndex > 0) {
    unitIndex -= 1;
    unit = COMPACT_UNITS[unitIndex];
    scaled = abs / unit.value;
  }

  const formatted = numberFormatter('en-US', {
    maximumFractionDigits,
  }).format(scaled);
  return `${sign}${formatted}${unit.suffix}`;
}

export function formatCompact(value) {
  return formatCompactNumber(value);
}

export function formatTokenAmount(value) {
  return formatCompactNumber(value);
}

export function formatCompactCurrency(value) {
  const amount = Number(value || 0);
  if (Math.abs(amount) < 1000) {
    return formatUsd(amount);
  }

  return `$${numberFormatter('en-US', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(amount)}`;
}

export function formatUsd(value) {
  return `$${Number(value || 0).toFixed(2)}`;
}

const dateTimeFormatterCache = new Map();

function dateTimeFormatter(timeZone) {
  const key = timeZone || '';
  let formatter = dateTimeFormatterCache.get(key);
  if (!formatter) {
    const options = {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hourCycle: 'h23',
    };
    if (timeZone) {
      options.timeZone = timeZone;
    }
    formatter = new Intl.DateTimeFormat('en-US', options);
    dateTimeFormatterCache.set(key, formatter);
  }
  return formatter;
}

function timestampParts(value, timeZone) {
  const raw = String(value ?? '').trim();
  // Date-only / month-only keys are calendar labels, not UTC instants.
  if (!raw || /^\d{4}-\d{2}(-\d{2})?$/.test(raw)) {
    return null;
  }
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) {
    return null;
  }
  const parts = {};
  for (const part of dateTimeFormatter(timeZone).formatToParts(date)) {
    if (part.type !== 'literal') {
      parts[part.type] = part.value;
    }
  }
  if (!parts.year || !parts.month || !parts.day || !parts.hour || !parts.minute) {
    return null;
  }
  return parts;
}

export function formatClock(value, timeZone) {
  const parts = timestampParts(value, timeZone);
  if (!parts) {
    return String(value || '--');
  }
  return `${parts.hour}:${parts.minute}`;
}

export function formatDateTime(value, timeZone) {
  const parts = timestampParts(value, timeZone);
  if (!parts) {
    return value ? String(value) : '--';
  }
  const base = `${parts.year}-${parts.month}-${parts.day} ${parts.hour}:${parts.minute}`;
  return parts.second && parts.second !== '00' ? `${base}:${parts.second}` : base;
}

export function formatMaybe(value, fallback = '尚未记录') {
  return value ? escapeHtml(value) : fallback;
}

export function formatPercent(value, total) {
  const numerator = Number(value || 0);
  const denominator = Number(total || 0);
  if (!Number.isFinite(numerator) || !Number.isFinite(denominator) || denominator <= 0) {
    return '0.0%';
  }
  return `${((numerator / denominator) * 100).toFixed(1)}%`;
}

export function truncate(value, max = 68) {
  const text = String(value || '');
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1)}…`;
}

export function shortLabel(value, max = 14) {
  const text = String(value || '');
  if (text.length <= max) return text;
  return `${text.slice(0, Math.max(3, max - 1))}…`;
}

export function statusTone(status) {
  const normalized = String(status || '').toLowerCase();
  if (
    normalized.includes('success') ||
    normalized.includes('ok') ||
    normalized.includes('ready') ||
    normalized.includes('active') ||
    normalized.includes('installed') ||
    normalized.includes('configured')
  ) {
    return 'good';
  }
  if (
    normalized.includes('warn') ||
    normalized.includes('fail') ||
    normalized.includes('error') ||
    normalized.includes('missing') ||
    normalized.includes('stale') ||
    normalized.includes('disabled')
  ) {
    return 'warn';
  }
  return 'neutral';
}

export function ratio(value, max) {
  if (!Number.isFinite(value) || !Number.isFinite(max) || max <= 0) return 0;
  return Math.max(0, Math.min(100, (value / max) * 100));
}
