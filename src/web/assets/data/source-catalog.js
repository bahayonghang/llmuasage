const FALLBACK_AGENT_LOGO_URL = 'assets/agent-logos/fallback.svg';
const AGENT_LOGO_URL_PATTERN = /^assets\/agent-logos\/[a-z0-9_-]+\.svg$/;
const SOURCE_ID_PATTERN = /^[a-z0-9][a-z0-9_-]*$/;

let cachedKey = null;
let cachedCatalog = [];

export function supportedSourceIds(value = '') {
  return String(value)
    .split(',')
    .map((source) => source.trim())
    .filter((source) => SOURCE_ID_PATTERN.test(source));
}

export function fallbackSourceDisplayName(stableId) {
  return String(stableId || '')
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function normalizeCatalogEntry(entry) {
  const id = String(entry?.id || '').trim();
  if (!SOURCE_ID_PATTERN.test(id)) return null;

  const displayName = String(entry?.display_name || '').trim() || fallbackSourceDisplayName(id);
  const requestedLogoUrl = String(entry?.logo_url || '').trim();
  const logoUrl = AGENT_LOGO_URL_PATTERN.test(requestedLogoUrl)
    ? requestedLogoUrl
    : FALLBACK_AGENT_LOGO_URL;
  return { id, display_name: displayName, logo_url: logoUrl };
}

export function parseSourceBadgeCatalog(rawCatalog, supportedSources = '') {
  const supportedIds = Array.isArray(supportedSources)
    ? supportedSources.filter((source) => SOURCE_ID_PATTERN.test(String(source)))
    : supportedSourceIds(supportedSources);
  const catalog = [];
  const seen = new Set();

  try {
    const parsed = typeof rawCatalog === 'string' ? JSON.parse(rawCatalog) : rawCatalog;
    if (Array.isArray(parsed)) {
      for (const candidate of parsed) {
        const entry = normalizeCatalogEntry(candidate);
        if (!entry || seen.has(entry.id)) continue;
        catalog.push(entry);
        seen.add(entry.id);
      }
    }
  } catch (_error) {
    // The compatibility body attribute remains the recovery source.
  }

  for (const id of supportedIds) {
    const stableId = String(id);
    if (seen.has(stableId)) continue;
    catalog.push({
      id: stableId,
      display_name: fallbackSourceDisplayName(stableId),
      logo_url: FALLBACK_AGENT_LOGO_URL,
    });
    seen.add(stableId);
  }
  return catalog;
}

export function readSourceBadgeCatalog() {
  const rawCatalog = document.getElementById('source-badge-catalog')?.textContent || '';
  const supportedSources = document.body?.dataset?.supportedSources || '';
  const key = `${rawCatalog}\u0000${supportedSources}`;
  if (key !== cachedKey) {
    cachedKey = key;
    cachedCatalog = parseSourceBadgeCatalog(rawCatalog, supportedSources);
  }
  return cachedCatalog;
}

export function sourceDisplayName(source, catalog = readSourceBadgeCatalog()) {
  const stableId = String(source || '').trim();
  const registered = catalog.find((entry) => entry.id === stableId)?.display_name;
  return registered || fallbackSourceDisplayName(stableId);
}
