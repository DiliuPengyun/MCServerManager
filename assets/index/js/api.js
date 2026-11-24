/* ================= API ================= */
async function api(path, opts) {
  opts = opts || {};
  opts.headers = Object.assign({
    'Content-Type': 'application/json'
  }, opts.headers || {});
  const res = await fetch(path, opts);
  let data = null;
  try { data = await res.json(); } catch (e) { /* 非 JSON 响应 */ }
  if (!res.ok) {
    const detail = data && data.error;
    const code = typeof detail === 'string'
      ? detail
      : (detail && typeof detail === 'object' && detail.code) ? detail.code : null;
    // 错误体通常同时带 error 代码与 message；优先展示 message。
    const message = (data && data.message) ||
      (typeof detail === 'string' ? detail : (detail && detail.message)) ||
      ('HTTP ' + res.status);
    const err = new Error(message);
    err.status = res.status;
    err.code = code || ('http_' + res.status);
    err.data = data;
    throw err;
  }
  return data;
}

// 所有服务器资源统一使用稳定 UUID；current 只作为列表的 DOM 下标。
// suffix 可包含路径和查询串，动态片段由调用方自行 encodeURIComponent。
function serverPath(id, suffix) {
  if (id == null || id === '') throw new Error('缺少服务器 UUID');
  const base = '/api/servers/' + encodeURIComponent(id);
  if (!suffix) return base;
  return base + (suffix.charAt(0) === '/' ? '' : '/') + suffix;
}

function operationPath(id) {
  if (id == null || id === '') throw new Error('缺少 operation ID');
  return '/api/operations/' + encodeURIComponent(id);
}
