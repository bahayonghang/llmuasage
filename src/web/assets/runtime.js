const logger = window.console;

let runtimeState = null;
let renderFn = null;

/*
 * ========================================================================
 * 步骤1：存放当前 dashboard 状态
 * ========================================================================
 * 目标：
 * 1) toggle 切换语言时不重新拉数据，直接复用最近一次 rawData
 * 2) 让 i18n 订阅器调用 rerender 即可
 */
export function setRuntimeState(state) {
  runtimeState = state;
}

export function getRuntimeState() {
  return runtimeState;
}

export function setRenderer(fn) {
  renderFn = typeof fn === 'function' ? fn : null;
}

export function rerender() {
  logger.info('开始触发 dashboard 重渲染');

  if (!runtimeState || !runtimeState.rawData || !renderFn) {
    logger.warn('runtime 状态尚未就绪，跳过重渲染');
    return;
  }

  renderFn(runtimeState.rawData);

  logger.info('完成 dashboard 重渲染');
}
