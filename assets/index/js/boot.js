/* ================= 启动与轮询 ================= */
function findServerIndex(id) {
  if (id == null) return -1;
  return servers.findIndex(server => server.id === id);
}

// 列表刷新后按 UUID 恢复选中项；找不到时退回第一台。
function restoreSelection(id) {
  const index = findServerIndex(id);
  if (index >= 0) {
    current = index;
    selectedServerId = servers[index].id;
    return true;
  }
  if (servers.length > 0) {
    current = 0;
    selectedServerId = servers[0].id;
  } else {
    current = -1;
    selectedServerId = null;
  }
  return false;
}

async function boot() {
  $('app').style.display = 'flex';
  try {
    servers = await api('/api/servers');
  } catch (e) {
    toast('无法连接服务器，请确认 web 服务正在运行', 'error');
    return;
  }
  if (servers.length > 0 && findServerIndex(selectedServerId) < 0) {
    current = -1;
    selectedServerId = null;
    await selectServer(0);
  } else {
    restoreSelection(selectedServerId);
    renderSidebar();
    renderMain();
  }
  startPolling();
}

function startPolling() {
  stopPolling();
  pollTimer = setInterval(refresh, 3000);
}
function stopPolling() {
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
}

async function refresh(force) {
  if (!force && modalDepth > 0) return; // 弹窗打开时暂停轮询
  const previousId = selectedServerId || (servers[current] && servers[current].id) || null;
  try {
    servers = await api('/api/servers');
  } catch (e) { return; }
  restoreSelection(previousId);
  const changed = previousId !== selectedServerId;
  // 选中项被外部移除时回退到第一台，并清理旧服务器的页内缓存。
  if (changed) resetServerScopedState();
  renderSidebar();
  renderMain();
  if (changed && selectedServerId) await loadTabData(currentTab);
}
