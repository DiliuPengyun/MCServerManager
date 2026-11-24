/* ================= 备份 ================= */
async function loadBackups() {
  const serverId = currentServerId();
  if (!serverId) return;
  try {
    const d = await api(serverPath(serverId, 'backups'));
    if (currentServerId() !== serverId) return;
    lastBackups = d.backups || [];
    backupsFor = serverId;
  } catch (e) {
    if (currentServerId() === serverId) toast('备份列表加载失败: ' + e.message, 'error');
  }
  renderBackupsPanel();
}

function renderBackupsPanel() {
  const s = servers[current];
  const box = $('backups-content');
  if (!s) { box.innerHTML = ''; return; }
  const stopped = s.state === 'STOPPED';
  let html = '<div class="card"><h3 class="card-title">备份信息</h3><div class="info-grid">' +
    '<div class="info-item"><div class="k">备份目录</div><div class="v mono">' + esc(s.backup_dir || '-') + '</div></div>' +
    '<div class="info-item"><div class="k">世界目录</div><div class="v mono">' + esc(s.world_dir || '-') + '</div></div>' +
    '</div></div>';
  if (s.state === 'UNKNOWN') {
    html += '<div class="inline-msg warn">服务器状态未知，无法确认 tmux 会话，不能创建或恢复备份</div>';
  } else if (!stopped) {
    html += '<div class="inline-msg warn">服务器正在运行，需先停止服务器才能创建或恢复备份</div>';
  }
  html += '<div class="panel-head"><h3>备份列表（' + lastBackups.length + '）</h3>' +
    '<button class="btn btn-primary" id="backup-create"' + (stopped ? '' : ' disabled') + '>创建备份</button></div>' +
    '<div id="backup-list"></div>';
  box.innerHTML = html;
  const listEl = $('backup-list');
  if (backupsFor !== currentServerId()) {
    listEl.innerHTML = '<div class="side-empty">加载中…</div>';
  } else if (lastBackups.length === 0) {
    listEl.innerHTML = '<div class="side-empty">暂无备份</div>';
  } else {
    lastBackups.forEach(name => {
      const row = document.createElement('div');
      row.className = 'backup-row';
      const nm = document.createElement('span');
      nm.className = 'bname';
      nm.textContent = name;
      const restore = document.createElement('button');
      restore.className = 'btn';
      restore.textContent = '恢复';
      restore.disabled = !stopped;
      restore.addEventListener('click', () => restoreBackup(name));
      const del = document.createElement('button');
      del.className = 'btn btn-danger';
      del.textContent = '删除';
      del.addEventListener('click', () => deleteBackup(name));
      row.appendChild(nm);
      row.appendChild(restore);
      row.appendChild(del);
      listEl.appendChild(row);
    });
  }
  $('backup-create').addEventListener('click', createBackup);
}

async function createBackup() {
  const serverId = currentServerId();
  if (!serverId) return;
  const btn = $('backup-create');
  if (btn) { btn.disabled = true; btn.textContent = '创建中…'; }
  try {
    const r = await api(serverPath(serverId, 'backups'), { method: 'POST', body: '{}' });
    toast('备份已创建: ' + (r.name || ''), 'success');
  } catch (e) {
    toast('创建备份失败: ' + e.message, 'error');
  }
  if (currentServerId() === serverId) await loadBackups();
}

async function restoreBackup(name) {
  const serverId = currentServerId();
  if (!serverId) return;
  if (!confirm('确定恢复备份「' + name + '」吗？\n当前世界数据将被该备份覆盖。')) return;
  try {
    await api(serverPath(serverId, 'backups/' + encodeURIComponent(name) + '/restore'),
      { method: 'POST', body: '{}' });
    toast('备份已恢复', 'success');
  } catch (e) {
    toast('恢复失败: ' + e.message, 'error');
  }
  if (currentServerId() === serverId) await loadBackups();
}

async function deleteBackup(name) {
  const serverId = currentServerId();
  if (!serverId) return;
  if (!confirm('确定删除备份「' + name + '」吗？此操作不可恢复。')) return;
  try {
    await api(serverPath(serverId, 'backups/' + encodeURIComponent(name)), { method: 'DELETE' });
    toast('备份已删除', 'success');
  } catch (e) {
    toast('删除失败: ' + e.message, 'error');
  }
  if (currentServerId() === serverId) await loadBackups();
}
