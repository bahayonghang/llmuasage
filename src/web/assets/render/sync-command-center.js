import { UI_COPY, getShellCopy } from '../copy.js';
import { escapeHtml, formatNumber } from '../data.js';

const logger = window.console;

function hasUsableCenter(center) {
  return Boolean(
    center &&
      (center.generated_at ||
        center.last_run ||
        center.current_job ||
        (center.sources || []).length > 0 ||
        Number(center.metrics?.stored_events || 0) > 0 ||
        Number(center.metrics?.events_seen || 0) > 0 ||
        Number(center.metrics?.inserted_delta || 0) > 0),
  );
}

function runningEventState(activeJobSnapshot) {
  const lastEvent = activeJobSnapshot?.last_event;
  if (activeJobSnapshot?.status !== 'running') return null;

  const kind = lastEvent?.type || lastEvent?.kind || lastEvent?.event || Object.keys(lastEvent || {})[0] || 'running';
  const summary = lastEvent?.summary || lastEvent?.Finished?.summary || null;
  const stats = lastEvent?.stats || lastEvent?.SourceFinished?.stats || null;
  const source = lastEvent?.source || lastEvent?.SourceFinished?.source || lastEvent?.Progress?.source || null;

  return { kind, source, summary, stats };
}

function structuredJobSnapshot(snapshot) {
  if (!snapshot?.status) return null;
  return {
    job_id: snapshot.job_id,
    status: snapshot.status,
    last_event: snapshot.last_event || null,
    started_at: snapshot.started_at,
    finished_at: snapshot.finished_at,
    error_key: snapshot.status === 'failed' ? 'syncCenter.reason.jobFailed' : null,
  };
}

function centerWithJobOverlay(center, snapshot) {
  const current_job = structuredJobSnapshot(snapshot);
  if (!current_job) return center;

  if (snapshot.status === 'running' || snapshot.status === 'cancelling') {
    return {
      ...(center || {}),
      tone: 'running',
      headline_key: 'syncCenter.headline.running',
      reason_key: snapshot.status === 'cancelling' ? 'syncCenter.reason.cancelling' : 'syncCenter.reason.running',
      current_job,
    };
  }

  if (snapshot.status === 'failed') {
    return {
      ...(center || {}),
      tone: 'warn',
      headline_key: 'syncCenter.headline.failed',
      reason_key: 'syncCenter.reason.failedJob',
      current_job,
    };
  }

  if (snapshot.status === 'cancelled') {
    return {
      ...(center || {}),
      tone: 'neutral',
      headline_key: 'syncCenter.headline.cancelled',
      reason_key: 'syncCenter.reason.cancelled',
      current_job,
    };
  }

  return center;
}

function displayKey(key, fallback) {
  return getShellCopy(key || '') || fallback || key || '--';
}

function metricCards(center, running) {
  const copy = UI_COPY.sections.syncCenter;
  const metrics = center?.metrics || {};
  const runningSummary = running?.summary || null;
  const runningStats = running?.stats || null;
  const values = {
    eventsSeen: runningSummary?.total_seen ?? runningStats?.events_seen ?? metrics.events_seen,
    insertedDelta: runningSummary?.total_inserted ?? runningStats?.events_inserted ?? metrics.inserted_delta,
    storedEvents: runningSummary?.stored_events ?? runningStats?.stored_events ?? metrics.stored_events,
    sourcesReady: metrics.sources_total ? `${formatNumber(metrics.sources_ready)} / ${formatNumber(metrics.sources_total)}` : '--',
  };

  return Object.entries(copy.metrics)
    .map(
      ([key, label]) => `
        <div class="sync-command-center-metric">
          <div class="mini-label">${escapeHtml(label)}</div>
          <strong>${escapeHtml(values[key] ?? '--')}</strong>
        </div>
      `,
    )
    .join('');
}

function safetyLine(center) {
  const copy = UI_COPY.sections.syncCenter;
  const safety = center?.safety || {};
  const risks = safety.risk_sources || [];
  if (risks.length > 0 || safety.lossy_rebuild_risk) {
    return `${copy.riskPrefix}${risks.length ? ` ${risks.map((item) => escapeHtml(item)).join(', ')}` : ''}`;
  }
  if (!hasUsableCenter(center)) {
    return copy.sourcesEmpty;
  }
  return copy.noRisk;
}


function sourceSegmentedBar(center) {
  const copy = UI_COPY.sections.syncCenter;
  const sources = center?.sources || [];
  const segments = sources
    .map((source) => {
      const rawShare = Number(source?.share || 0);
      const share = Number.isFinite(rawShare) ? Math.max(0, Math.min(1, rawShare)) : 0;
      return { source, share };
    })
    .filter((item) => item.share > 0);

  if (segments.length === 0) return '';

  return `
    <div class="sync-command-center-segmented-bar" role="list" aria-label="${escapeHtml(copy.sourceShareAria)}">
      ${segments
        .map(({ source, share }) => {
          const tone = source.lossy_rebuild_risk ? 'warn' : source.tone || 'neutral';
          const percent = Math.max(1, share * 100);
          return `
            <span
              class="sync-command-center-segment"
              role="listitem"
              data-source="${escapeHtml(source.source || '--')}"
              data-tone="${escapeHtml(tone)}"
              style="--segment-share: ${percent.toFixed(2)}%;"
              title="${escapeHtml(`${source.source || '--'} ${(share * 100).toFixed(1)}%`)}"
            ></span>
          `;
        })
        .join('')}
    </div>
  `;
}

function sourceCards(center) {
  const copy = UI_COPY.sections.syncCenter;
  const sources = center?.sources || [];
  if (sources.length === 0) {
    return `<div class="sync-command-center-empty">${escapeHtml(copy.sourcesEmpty)}</div>`;
  }

  return sources
    .map((source) => {
      const status = copy.sourceStatus[source.status] || source.status || '--';
      const tone = source.lossy_rebuild_risk ? 'warn' : source.tone || 'neutral';
      return `
        <article class="sync-command-center-source" data-tone="${escapeHtml(tone)}">
          <div class="sync-command-center-source-head">
            <strong>${escapeHtml(source.source || '--')}</strong>
            <span>${escapeHtml(status)}</span>
          </div>
          <div class="sync-command-center-source-body">
            <span>${escapeHtml(copy.metrics.eventsSeen)} ${formatNumber(source.events_seen)}</span>
            <span>${escapeHtml(copy.metrics.insertedDelta)} ${formatNumber(source.events_inserted)}</span>
            <span>${escapeHtml(copy.metrics.storedEvents)} ${formatNumber(source.stored_events)}</span>
          </div>
          ${source.error_key ? `<div class="sync-command-center-source-error">${escapeHtml(displayKey(source.error_key))}</div>` : ''}
        </article>
      `;
    })
    .join('');
}


function shortId(value) {
  const text = String(value || '');
  if (text.length <= 14) return text || '--';
  return `${text.slice(0, 8)}…${text.slice(-4)}`;
}

function eventLabel(event) {
  if (!event) return '--';
  if (typeof event === 'string') return event || '--';
  return event.type || event.kind || event.event || Object.keys(event || {})[0] || '--';
}

function summaryRows(center, activeJobSnapshot, running) {
  const labels = UI_COPY.sections.syncCenter.statusLabels;
  const current_job = center?.current_job || null;
  const last_run = center?.last_run || null;
  const current = activeJobSnapshot || current_job;
  const currentLastEvent = activeJobSnapshot?.last_event || current_job?.last_event || null;
  const rows = [];

  if (current) {
    rows.push({ label: labels.currentStatus, value: current.status || (running ? 'running' : '--') });
    rows.push({ label: labels.jobId, value: shortId(current.job_id || current.id) });
    rows.push({ label: labels.lastEvent, value: eventLabel(currentLastEvent) });
    rows.push({ label: labels.started, value: current.started_at || '--' });
    if (current['error_key']) rows.push({ label: labels.error, value: displayKey(current['error_key']) });
  }

  if (last_run) {
    rows.push({ label: labels.lastCommand, value: last_run.command || '--' });
    rows.push({ label: labels.lastStatus, value: last_run.status || '--' });
    rows.push({ label: labels.lastFinished, value: last_run.finished_at || last_run.started_at || '--' });
    if (last_run['error_key']) rows.push({ label: labels.lastError, value: displayKey(last_run['error_key']) });
  }

  if (rows.length === 0) return '';

  return `
    <div class="sync-command-center-status" data-running="${running ? 'true' : 'false'}">
      ${rows
        .map(
          (row) => `
            <div class="sync-command-center-status-row">
              <span>${escapeHtml(row.label)}</span>
              <strong>${escapeHtml(row.value)}</strong>
            </div>
          `,
        )
        .join('')}
    </div>
  `;
}

function secondaryStatus(center, activeJobSnapshot, running) {
  const last_run = center?.last_run || null;
  const current_job = center?.current_job || null;
  const current = activeJobSnapshot || current_job;
  if (running) {
    const status = current?.status || 'running';
    const event = eventLabel(current?.last_event || activeJobSnapshot?.last_event || current_job?.last_event);
    return `
      <div class="sync-command-center-secondary" data-state="running">
        <div class="sync-command-center-secondary-copy">
          <strong>${escapeHtml(getShellCopy('shell.sync.running'))}</strong>
          <span>${escapeHtml(status)} · ${escapeHtml(event)}</span>
        </div>
        <button class="btn" type="button" data-sync-command-center-action="sync-secondary">
          ${escapeHtml(getShellCopy('shell.sync.cancel'))}
        </button>
      </div>
    `;
  }

  if (current?.status === 'failed' || current?.status === 'cancelled') {
    const event = eventLabel(current?.last_event || activeJobSnapshot?.last_event || current_job?.last_event);
    return `
      <div class="sync-command-center-secondary" data-state="${escapeHtml(current.status)}">
        <div class="sync-command-center-secondary-copy">
          <strong>${escapeHtml(current.status)}</strong>
          <span>${escapeHtml(event)} · ${escapeHtml(current.finished_at || current.started_at || '--')}</span>
          ${current['error_key'] ? `<span>${escapeHtml(displayKey(current['error_key']))}</span>` : ''}
        </div>
      </div>
    `;
  }

  if (!last_run) return '';
  return `
    <div class="sync-command-center-secondary" data-state="last-run">
      <div class="sync-command-center-secondary-copy">
        <strong>${escapeHtml(last_run.status || '--')}</strong>
        <span>${escapeHtml(last_run.command || '--')} · ${escapeHtml(last_run.finished_at || last_run.started_at || '--')}</span>
        ${last_run['error_key'] ? `<span>${escapeHtml(displayKey(last_run['error_key']))}</span>` : ''}
      </div>
    </div>
  `;
}

function actionButton(center, running) {
  const copy = UI_COPY.sections.syncCenter;
  const action = (center?.actions || []).find((item) => item.id === 'sync') || { label_key: 'syncCenter.action.sync' };
  const disabled = Boolean(action.disabled && !running);
  const reason = action.reason_key ? displayKey(action.reason_key, '') : '';
  return `
    <div class="sync-command-center-action">
      <button class="btn ${action.primary !== false ? 'btn-primary' : ''}" type="button" data-sync-command-center-action="sync" ${disabled ? 'disabled' : ''}>
        ${escapeHtml(running ? getShellCopy('shell.sync.cancel') : displayKey(action.label_key, copy.actions.sync))}
      </button>
      ${reason ? `<span>${escapeHtml(reason)}</span>` : ''}
    </div>
  `;
}

function emptyCenter(host) {
  host.innerHTML = `
    <div class="sync-command-center-empty">
      <div class="section-eyebrow">${escapeHtml(getShellCopy('shell.syncCenter.eyebrow'))}</div>
      <div>${escapeHtml(getShellCopy('shell.syncCenter.loading'))}</div>
    </div>
  `;
}

/*
 * ========================================================================
 * 步骤1：渲染同步命令中心
 * ========================================================================
 * 目标：
 * 1) 只消费后端 sync_command_center 与 job last_event 的结构化字段
 * 2) 运行中叠加 last_event.summary / last_event.stats，不解析 human summary 文本
 * 3) action 复用现有 #btn-sync 生命周期，不直接调用 jobs API
 * 4) 缺少真实数据时展示空态，避免把 fallback null 误读成已验证安全
 */
export function renderSyncCommandCenter(context, state) {
  logger.info('开始渲染同步命令中心');

  const host = document.getElementById('sync-command-center');
  if (!host) return;

  const activeJobSnapshot = state?.activeJobSnapshot;
  const center = centerWithJobOverlay(context?.syncCommandCenter, activeJobSnapshot);
  const running = runningEventState(center?.current_job || activeJobSnapshot);
  if (!hasUsableCenter(center) && !running) {
    emptyCenter(host);
    logger.info('完成同步命令中心空态渲染');
    return;
  }

  const headlineKey = center?.headline_key;
  const reasonKey = center?.reason_key;
  const tone = center?.tone || 'neutral';
  const copy = UI_COPY.sections.syncCenter;
  const workerLock = copy.workerLockState[center?.safety?.worker_lock] || center?.safety?.worker_lock || '--';

  host.innerHTML = `
    <div class="sync-command-center-main" data-tone="${escapeHtml(tone)}">
      <div class="sync-command-center-copy">
        <div class="section-eyebrow">${escapeHtml(copy.eyebrow)}</div>
        <h2>${escapeHtml(displayKey(headlineKey, getShellCopy('syncCenter.headline.empty')))}</h2>
        <p>${escapeHtml(displayKey(reasonKey, getShellCopy('syncCenter.reason.empty')))}</p>
        <div class="sync-command-center-meta">
          <span>${escapeHtml(copy.generatedAt)} <strong>${escapeHtml(center?.generated_at || '--')}</strong></span>
          <span>${escapeHtml(copy.workerLock)} <strong>${escapeHtml(workerLock)}</strong></span>
          <span>${escapeHtml(safetyLine(center))}</span>
          ${running?.source ? `<span>${escapeHtml(running.source)}</span>` : ''}
        </div>
      </div>
      ${actionButton(center, running)}
    </div>
    ${secondaryStatus(center, activeJobSnapshot, running)}
    ${summaryRows(center, activeJobSnapshot, running)}
    <div class="sync-command-center-metrics">${metricCards(center, running)}</div>
    ${sourceSegmentedBar(center)}
    <div class="sync-command-center-sources">${sourceCards(center)}</div>
  `;

  host.querySelectorAll('[data-sync-command-center-action]').forEach((button) => {
    button.addEventListener('click', () => {
      document.getElementById('btn-sync')?.click();
    });
  });

  logger.info('完成同步命令中心渲染');
}
