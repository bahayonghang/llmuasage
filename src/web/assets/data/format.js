export function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

export function formatNumber(value) {
  return Intl.NumberFormat('en-US').format(Number(value || 0));
}

export function formatCompact(value) {
  return Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(Number(value || 0));
}

export function formatCompactCurrency(value) {
  const amount = Number(value || 0);
  if (Math.abs(amount) < 1000) {
    return formatUsd(amount);
  }

  return `$${Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(amount)}`;
}

export function formatUsd(value) {
  return `$${Number(value || 0).toFixed(2)}`;
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
