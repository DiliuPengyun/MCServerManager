/* ================= 全局设置弹窗 ================= */
async function openSettingsModal() {
  openModal('modal-settings');
  $('set-msg').className = 'inline-msg';
  $('set-folder-hint').textContent = '';
  $('set-java-hint').textContent = '';
  try {
    const d = await api('/api/settings');
    $('set-folder').value = d.servers_parent_folder || '';
    $('set-folder').placeholder = d.effective_parent_folder || '默认目录';
    $('set-folder-hint').textContent = '只影响以后新建和上传的服务器，不移动或改写已有服务器目录。当前生效：' + (d.effective_parent_folder || '-');
    $('set-java').value = d.global_java_path || '';
    $('set-java').placeholder = d.effective_java_path || 'java';
    $('set-java-hint').textContent = '留空使用 PATH 中的 java。当前生效：' + (d.effective_java_path || '-');
    $('set-port').value = d.web_port != null ? d.web_port : '';
  } catch (e) {
    setInlineMsg('set-msg', 'err', '设置加载失败: ' + e.message);
  }
}

async function saveSettings() {
  const port = parseInt($('set-port').value, 10);
  if (isNaN(port) || port <= 0 || port > 65535) {
    setInlineMsg('set-msg', 'err', 'Web 端口无效（1-65535）');
    return;
  }
  const body = {
    servers_parent_folder: $('set-folder').value.trim(),
    global_java_path: $('set-java').value.trim(),
    web_port: port
  };
  const btn = $('set-save');
  btn.disabled = true;
  try {
    const r = await api('/api/settings', { method: 'PUT', body: JSON.stringify(body) });
    setInlineMsg('set-msg', 'ok', '已保存' + (r.note ? '：' + r.note : ''));
    toast('全局设置已保存', 'success');
  } catch (e) {
    setInlineMsg('set-msg', 'err', '保存失败: ' + e.message);
  } finally {
    btn.disabled = false;
  }
}
