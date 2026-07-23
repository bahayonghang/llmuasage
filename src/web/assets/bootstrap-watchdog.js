(function () {
  'use strict';

  var CLAIM_TIMEOUT_MS = 3000;
  var READY_TIMEOUT_MS = 1000;
  var PROBE_TIMEOUT_MS = 1500;
  var state = 'waiting';
  var diagnosing = false;
  var claimTimer = null;
  var readyTimer = null;
  var earlyError = null;
  var mode = document.body && document.body.dataset.mode === 'snapshot' ? 'snapshot' : 'live';

  var COPY = {
    zh: {
      eyebrow: '看板启动',
      bootstrapTitle: '页面应用未能启动',
      bootstrapBody: '本地服务仍可访问，但页面资源加载失败。请刷新页面；若问题持续，请重启 llmusage serve。',
      serviceTitle: '本地服务不可用',
      serviceBody: '页面无法连接本地 llmusage 服务。请回到终端重启 llmusage serve。',
      snapshotTitle: '离线页面资源未能启动',
      snapshotBody: '快照数据仍在当前文件中，但页面脚本未能加载。请重新打开或重新导出快照。',
      retry: '刷新页面',
    },
    en: {
      eyebrow: 'Dashboard startup',
      bootstrapTitle: 'The page application did not start',
      bootstrapBody: 'The local service is reachable, but a page resource failed to load. Refresh the page, then restart llmusage serve if the problem continues.',
      serviceTitle: 'Local service unavailable',
      serviceBody: 'The page cannot reach the local llmusage service. Return to the terminal and restart llmusage serve.',
      snapshotTitle: 'Offline page resources did not start',
      snapshotBody: 'The snapshot data is still in this file, but its page scripts did not load. Reopen or export the snapshot again.',
      retry: 'Refresh page',
    },
  };

  function localeCopy() {
    return COPY[document.documentElement.dataset.locale === 'en' ? 'en' : 'zh'];
  }

  function clearTimers() {
    if (claimTimer !== null) window.clearTimeout(claimTimer);
    if (readyTimer !== null) window.clearTimeout(readyTimer);
    claimTimer = null;
    readyTimer = null;
  }

  function removeEarlyListeners() {
    window.removeEventListener('error', onEarlyError, true);
    window.removeEventListener('unhandledrejection', onEarlyRejection);
  }

  function escapeHtml(value) {
    return String(value || '')
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }

  function renderFailure(kind, detail) {
    if (state === 'ready') return;
    state = 'failed';
    diagnosing = false;
    clearTimers();

    var copy = localeCopy();
    var title = kind === 'service' ? copy.serviceTitle : (kind === 'snapshot' ? copy.snapshotTitle : copy.bootstrapTitle);
    var body = kind === 'service' ? copy.serviceBody : (kind === 'snapshot' ? copy.snapshotBody : copy.bootstrapBody);
    var host = document.getElementById('sync-command-center');
    if (host) {
      host.innerHTML = '<div class="dashboard-load-instrument" data-phase="error" data-tone="error" role="status" aria-live="polite">'
        + '<div class="section-eyebrow">' + escapeHtml(copy.eyebrow) + '</div>'
        + '<strong>' + escapeHtml(title) + '</strong>'
        + '<p>' + escapeHtml(body) + '</p>'
        + (detail ? '<span class="dashboard-load-detail">' + escapeHtml(detail) + '</span>' : '')
        + '<button class="btn btn-primary" type="button" data-dashboard-retry>' + escapeHtml(copy.retry) + '</button>'
        + '</div>';
    }

    var status = document.getElementById('status-panel');
    if (status) {
      status.innerHTML = '<div class="bootstrap-error" role="status">' + escapeHtml(title) + ': ' + escapeHtml(body) + '</div>';
    }
  }

  async function diagnose(detail) {
    if (state === 'ready' || diagnosing) return;
    diagnosing = true;
    if (mode === 'snapshot') {
      renderFailure('snapshot', detail);
      return;
    }

    var controller = new AbortController();
    var timer = window.setTimeout(function () { controller.abort(); }, PROBE_TIMEOUT_MS);
    try {
      var response = await window.fetch('/', { cache: 'no-store', signal: controller.signal });
      renderFailure(response.ok ? 'bootstrap' : 'service', detail);
    } catch (_) {
      renderFailure('service', detail);
    } finally {
      window.clearTimeout(timer);
    }
  }

  function errorDetail(value) {
    return value && value.message ? value.message : '';
  }

  function onEarlyError(event) {
    if (state === 'ready') return;
    earlyError = errorDetail(event && (event.error || event));
    if (state === 'claimed') void diagnose(earlyError);
  }

  function onEarlyRejection(event) {
    if (state === 'ready') return;
    earlyError = errorDetail(event && event.reason);
    if (state === 'claimed') void diagnose(earlyError);
  }

  window.__LLMUSAGE_BOOTSTRAP__ = {
    claim: function () {
      if (state === 'ready' || state === 'claimed') return;
      state = 'claimed';
      if (claimTimer !== null) window.clearTimeout(claimTimer);
      claimTimer = null;
      readyTimer = window.setTimeout(function () { void diagnose(earlyError); }, READY_TIMEOUT_MS);
    },
    ready: function () {
      if (state === 'ready') return;
      state = 'ready';
      diagnosing = false;
      clearTimers();
      removeEarlyListeners();
    },
    state: function () { return state; },
  };

  window.addEventListener('error', onEarlyError, true);
  window.addEventListener('unhandledrejection', onEarlyRejection);
  document.addEventListener('click', function (event) {
    if (state !== 'ready' && event.target && event.target.closest && event.target.closest('[data-dashboard-retry]')) {
      window.location.reload();
    }
  });
  claimTimer = window.setTimeout(function () { void diagnose(earlyError); }, CLAIM_TIMEOUT_MS);
}());
