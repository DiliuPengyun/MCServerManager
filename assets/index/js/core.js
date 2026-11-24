'use strict';
/* ================= 工具 ================= */
const $ = id => document.getElementById(id);
const esc = s => String(s == null ? '' : s).replace(/[&<>"']/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

// logShown 是已消费的绝对行号；后端裁剪日志后仍只追加真正的新行。
// operation 和上传 task 共用同一套 log/log_offset 语义。
function appendTaskLogLines(task, data, append) {
  const log = Array.isArray(data.log) ? data.log : [];
  const offset = Number.isSafeInteger(data.log_offset) && data.log_offset >= 0 ? data.log_offset : 0;
  const start = Math.max(task.logShown, offset);
  for (let i = start; i < offset + log.length; ++i) append(log[i - offset]);
  task.logShown = Math.max(task.logShown, offset + log.length);
}

// 当前选中服务器的稳定 UUID；current 只用于 DOM 下标。
function currentServerId() {
  if (typeof servers === 'undefined' || !servers[current]) return null;
  return servers[current].id || null;
}

function serverNameById(id) {
  if (typeof servers === 'undefined' || id == null) return '';
  const server = servers.find(item => item.id === id);
  return server ? server.name : '';
}

// 轮询 operation 直到完成；运行中的日志通过 onUpdate 交给调用方增量消费。
// 失败时抛出带 code 的异常，保留 operation.error.code 供生命周期分派。
async function waitOperation(initial, options) {
  const settings = options || {};
  const interval = Number.isFinite(settings.interval) ? Math.max(0, settings.interval) : 300;
  let operation = initial;
  while (operation && operation.status === 'running') {
    if (settings.onUpdate) settings.onUpdate(operation);
    await new Promise(resolve => setTimeout(resolve, interval));
    operation = await api(operationPath(operation.operation_id));
  }
  if (settings.onUpdate && operation) settings.onUpdate(operation);
  if (!operation || operation.status !== 'done') {
    const detail = (operation && operation.error) || {};
    const err = new Error(detail.message || '后台操作未完成');
    err.code = detail.code || (operation && operation.status === 'error'
      ? 'operation_failed' : 'operation_incomplete');
    err.operation = operation || null;
    throw err;
  }
  return operation.result || {};
}
