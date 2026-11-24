/* ================= 服务器设置 / 删除 ================= */
function fillServerForm() {
  const s = servers[current];
  if (!s) return;
  $('sv-name').value = s.name || '';
  $('sv-java').value = s.java_path || '';
  $('sv-min').value = s.min_memory_mb != null ? s.min_memory_mb : '';
  $('sv-max').value = s.max_memory_mb != null ? s.max_memory_mb : '';
  $('sv-jvm').value = s.additional_jvm_args || '';
  $('sv-backup').value = s.backup_dir || '';
  $('sv-world').value = s.world_dir || '';
  $('sv-working').value = s.working_dir || '';
  $('sv-location').value = '';
  $('sv-msg').className = 'inline-msg';
}

async function saveServerConfig() {
  const s = servers[current];
  if (!s) return;
  const serverId = s.id;
  const name = $('sv-name').value.trim();
  const minMem = parseInt($('sv-min').value, 10);
  const maxMem = parseInt($('sv-max').value, 10);
  if (!name) { setInlineMsg('sv-msg', 'err', '服务器名称不能为空'); return; }
  if (isNaN(minMem) || isNaN(maxMem) || minMem < 0 || maxMem < 0) {
    setInlineMsg('sv-msg', 'err', '内存配置无效，请输入不小于 0 的数字');
    return;
  }
  if (maxMem < minMem) { setInlineMsg('sv-msg', 'err', '最大内存不能小于最小内存'); return; }
  const body = {
    name: name,
    java_path: $('sv-java').value.trim(),
    additional_jvm_args: $('sv-jvm').value,
    backup_dir: $('sv-backup').value.trim(),
    world_dir: $('sv-world').value.trim(),
    min_memory_mb: minMem,
    max_memory_mb: maxMem
  };
  const btn = $('sv-save');
  btn.disabled = true;
  try {
    await api(serverPath(serverId, 'config'), { method: 'PUT', body: JSON.stringify(body) });
    if (currentServerId() === serverId) setInlineMsg('sv-msg', 'ok', '设置已保存');
    toast('服务器设置已保存', 'success');
    await refresh();
  } catch (e) {
    if (currentServerId() === serverId) setInlineMsg('sv-msg', 'err', '保存失败: ' + e.message);
    else toast('保存失败: ' + e.message, 'error');
  } finally {
    btn.disabled = false;
  }
}

function newIdempotencyKey() {
  if (window.crypto && typeof window.crypto.randomUUID === 'function') {
    return window.crypto.randomUUID();
  }
  // 局域网 HTTP 不一定属于安全上下文；幂等键只需唯一，不作为安全凭据。
  const bytes = new Uint8Array(16);
  if (window.crypto && typeof window.crypto.getRandomValues === 'function') {
    window.crypto.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
  return hex.slice(0, 8) + '-' + hex.slice(8, 12) + '-' + hex.slice(12, 16) + '-' +
    hex.slice(16, 20) + '-' + hex.slice(20);
}

async function changeServerLocation(moveFiles) {
  const s = servers[current];
  if (!s) return;
  const serverId = s.id;
  const path = $('sv-location').value.trim();
  if (!path || !path.startsWith('/')) {
    setInlineMsg('sv-msg', 'err', '目标目录必须是完整的绝对路径');
    return;
  }
  if (s.state !== 'STOPPED') {
    setInlineMsg('sv-msg', 'err', '请先停止服务器，再操作工作目录');
    return;
  }
  const prompt = moveFiles
    ? '由管理器将整个工作目录移动到：\n' + path + '\n\n目标必须不存在；跨文件系统时会先复制并校验。是否继续？'
    : '确认你已经手动把服务器文件搬迁到：\n' + path + '\n\n本操作只修改注册路径，不复制或删除文件。是否继续？';
  if (!confirm(prompt)) return;
  const buttons = [$('sv-move'), $('sv-relocate')];
  buttons.forEach(button => button.disabled = true);
  try {
    const operation = await api(serverPath(serverId, moveFiles ? 'move' : 'relocate'), {
      method: 'POST',
      headers: { 'Idempotency-Key': newIdempotencyKey() },
      body: JSON.stringify({ path: path })
    });
    const result = await waitOperation(operation);
    await refresh();
    const message = result.warning || (moveFiles ? '目录已移动' : '已重新定位');
    if (currentServerId() === serverId) {
      fillServerForm();
      setInlineMsg('sv-msg', result.warning ? 'err' : 'ok', message);
    }
    toast(result.warning || '服务器目录已更新', result.warning ? 'error' : 'success');
  } catch (e) {
    if (currentServerId() === serverId) {
      setInlineMsg('sv-msg', 'err', '目录操作失败: ' + e.message);
    } else {
      toast('目录操作失败: ' + e.message, 'error');
    }
  } finally {
    buttons.forEach(button => button.disabled = false);
  }
}

async function removeServerRegistration() {
  const s = servers[current];
  if (!s) return;
  const serverId = s.id;
  if (s.state !== 'STOPPED') { toast('请先停止服务器', 'error'); return; }
  if (!confirm('仅从管理器移除「' + s.name + '」？\n\n工作目录及全部文件将保留：\n' + s.working_dir)) return;
  try {
    const operation = await api(serverPath(serverId), { method: 'DELETE' });
    await waitOperation(operation);
    toast('已移除管理，磁盘文件未改动', 'success');
  } catch (e) {
    toast('移除失败: ' + e.message, 'error');
    return;
  }
  if (currentServerId() === serverId) {
    current = -1;
    selectedServerId = null;
    closeStream();
  }
  await refresh();
}

async function deleteServer() {
  const s = servers[current];
  if (!s) return;
  const serverId = s.id;
  if (s.state !== 'STOPPED') { toast('删除磁盘文件前请先停止服务器', 'error'); return; }
  if (!confirm('确定删除服务器「' + s.name + '」吗？')) return;
  if (!confirm('最后确认：永久删除磁盘上的全部文件：\n' + (s.working_dir || '') + '\n\n此操作不可恢复！')) return;
  try {
    const query = '?delete_files=true&confirm=' + encodeURIComponent(s.working_dir || '');
    const operation = await api(serverPath(serverId) + query, { method: 'DELETE' });
    await waitOperation(operation);
    toast('服务器及磁盘文件已删除', 'success');
  } catch (e) {
    toast('删除失败: ' + e.message, 'error');
    return;
  }
  if (currentServerId() === serverId) {
    current = -1;
    selectedServerId = null;
    closeStream();
  }
  await refresh();
}
