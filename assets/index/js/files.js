/* ================= 文件管理 ================= */
function fmtSize(n) {
  if (n == null) return '';
  if (n < 1024) return n + ' B';
  if (n < 1024 * 1024) return (n / 1024).toFixed(1) + ' KiB';
  if (n < 1024 * 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + ' MiB';
  return (n / 1024 / 1024 / 1024).toFixed(1) + ' GiB';
}

function fileErrMsg(e) {
  return (e.data && e.data.message) ? e.data.message : e.message;
}

function joinRel(dir, name) { return dir ? dir + '/' + name : name; }

async function loadFiles() {
  const serverId = currentServerId();
  if (!serverId) return;
  const path = filesPath;
  try {
    const d = await api(serverPath(serverId, 'files?path=' + encodeURIComponent(path)));
    if (currentServerId() !== serverId || filesPath !== path) return;
    filesEntries = d.entries || [];
    setInlineMsg('files-msg', '', '');
  } catch (e) {
    if (currentServerId() === serverId) setInlineMsg('files-msg', 'err', '目录加载失败: ' + fileErrMsg(e));
  }
  renderFiles();
}

function renderFiles() {
  const editing = fileEditing !== null;
  $('files-list-wrap').style.display = editing ? 'none' : '';
  $('files-editor').style.display = editing ? '' : 'none';
  if (editing) return;
  $('files-path').textContent = '/' + (filesPath ? filesPath + '/' : '');
  $('files-up').disabled = !filesPath;
  const tbody = $('files-tbody');
  tbody.innerHTML = '';
  if (filesPath) {
    // 「..」返回上级行
    const tr = document.createElement('tr');
    const td = document.createElement('td');
    td.colSpan = 4;
    td.className = 'f-name link';
    td.textContent = '../';
    td.addEventListener('click', navUp);
    tr.appendChild(td);
    tbody.appendChild(tr);
  }
  if (filesEntries.length === 0) {
    const tr = document.createElement('tr');
    const td = document.createElement('td');
    td.colSpan = 4;
    td.className = 'f-meta';
    td.textContent = '空目录';
    tr.appendChild(td);
    tbody.appendChild(tr);
  }
  filesEntries.forEach(entry => {
    const tr = document.createElement('tr');
    const nameTd = document.createElement('td');
    nameTd.className = 'f-name link';
    nameTd.textContent = entry.is_dir ? entry.name + '/' : entry.name;
    nameTd.addEventListener('click', () => {
      if (entry.is_dir) navInto(entry.name); else openFile(entry.name);
    });
    const sizeTd = document.createElement('td');
    sizeTd.className = 'f-meta';
    sizeTd.textContent = entry.is_dir ? '-' : fmtSize(entry.size);
    const timeTd = document.createElement('td');
    timeTd.className = 'f-meta';
    timeTd.textContent = entry.mod_time || '';
    const opsTd = document.createElement('td');
    opsTd.className = 'f-ops';
    const openBtn = document.createElement('button');
    openBtn.className = 'btn';
    openBtn.textContent = entry.is_dir ? '打开' : '编辑';
    openBtn.addEventListener('click', () => {
      if (entry.is_dir) navInto(entry.name); else openFile(entry.name);
    });
    const renameBtn = document.createElement('button');
    renameBtn.className = 'btn';
    renameBtn.textContent = '重命名';
    renameBtn.addEventListener('click', () => openRenameModal(entry.name));
    const delBtn = document.createElement('button');
    delBtn.className = 'btn btn-danger';
    delBtn.textContent = '删除';
    delBtn.addEventListener('click', () => deleteEntry(entry));
    opsTd.appendChild(openBtn);
    opsTd.appendChild(renameBtn);
    opsTd.appendChild(delBtn);
    tr.appendChild(nameTd);
    tr.appendChild(sizeTd);
    tr.appendChild(timeTd);
    tr.appendChild(opsTd);
    tbody.appendChild(tr);
  });
}

function navInto(name) {
  ++fileReadSerial;
  filesPath = joinRel(filesPath, name);
  loadFiles();
}

function navUp() {
  if (!filesPath) return;
  ++fileReadSerial;
  const parts = filesPath.split('/');
  parts.pop();
  filesPath = parts.join('/');
  loadFiles();
}

async function openFile(name) {
  const serverId = currentServerId();
  if (!serverId) return;
  const rel = joinRel(filesPath, name);
  const request = ++fileReadSerial;
  try {
    const d = await api(serverPath(serverId, 'files/content?path=' + encodeURIComponent(rel)));
    if (currentServerId() !== serverId || request !== fileReadSerial) return;
    fileEditing = rel;
    $('files-edit-name').textContent = '/' + rel;
    $('files-text').value = d.content || '';
    setInlineMsg('files-msg', '', '');
    renderFiles();
  } catch (e) {
    if (currentServerId() === serverId && request === fileReadSerial) {
      setInlineMsg('files-msg', 'err', '打开失败: ' + fileErrMsg(e));
    }
  }
}

async function saveFile() {
  if (fileEditing === null) return;
  const serverId = currentServerId();
  if (!serverId) return;
  const rel = fileEditing;
  const btn = $('files-save');
  btn.disabled = true;
  try {
    await api(serverPath(serverId, 'files/content'), {
      method: 'PUT',
      body: JSON.stringify({ path: rel, content: $('files-text').value })
    });
    toast('文件已保存', 'success');
  } catch (e) {
    toast('保存失败: ' + fileErrMsg(e), 'error');
  } finally {
    btn.disabled = false;
  }
}

function closeEditor() {
  ++fileReadSerial;
  fileEditing = null;
  renderFiles();
  loadFiles();
}

// 打开文件操作弹窗（新建文件 / 新建文件夹 / 重命名共用）
function openFileModal(action) {
  fileModalAction = action;
  $('file-modal-title').textContent = action.title;
  $('file-modal-label').textContent = action.label || '名称';
  $('file-modal-input').value = action.value || '';
  setInlineMsg('file-modal-msg', '', '');
  openModal('modal-file');
  setTimeout(() => $('file-modal-input').focus(), 0);
}

function openCreateModal(isDir) {
  const serverId = currentServerId();
  if (!serverId) return;
  const basePath = filesPath;
  openFileModal({
    serverId: serverId,
    basePath: basePath,
    title: isDir ? '新建文件夹' : '新建文件',
    label: isDir ? '文件夹名' : '文件名',
    onOk: async name => {
      await api(serverPath(serverId, 'files'), {
        method: 'POST',
        body: JSON.stringify({ path: joinRel(basePath, name), is_dir: isDir })
      });
      toast(isDir ? '文件夹已创建' : '文件已创建', 'success');
    }
  });
}

function openRenameModal(name) {
  const serverId = currentServerId();
  if (!serverId) return;
  const basePath = filesPath;
  openFileModal({
    serverId: serverId,
    basePath: basePath,
    title: '重命名「' + name + '」',
    label: '新名称',
    value: name,
    onOk: async newName => {
      await api(serverPath(serverId, 'files/rename'), {
        method: 'PUT',
        body: JSON.stringify({ path: joinRel(basePath, name), new_name: newName })
      });
      toast('已重命名', 'success');
    }
  });
}

async function confirmFileModal() {
  if (!fileModalAction) return;
  const name = $('file-modal-input').value.trim();
  if (!name) {
    setInlineMsg('file-modal-msg', 'err', '名称不能为空');
    return;
  }
  const action = fileModalAction;
  const btn = $('file-modal-ok');
  btn.disabled = true;
  try {
    await action.onOk(name);
    closeModal('modal-file');
    fileModalAction = null;
    await loadFiles();
  } catch (e) {
    setInlineMsg('file-modal-msg', 'err', '操作失败: ' + fileErrMsg(e));
  } finally {
    btn.disabled = false;
  }
}

async function deleteEntry(entry) {
  const serverId = currentServerId();
  if (!serverId) return;
  const basePath = filesPath;
  const what = entry.is_dir ? '文件夹（含其中全部内容）' : '文件';
  if (!confirm('确定删除' + what + '「' + entry.name + '」吗？此操作不可恢复。')) return;
  try {
    await api(serverPath(serverId, 'files?path=' + encodeURIComponent(joinRel(basePath, entry.name))),
      { method: 'DELETE' });
    toast('已删除', 'success');
  } catch (e) {
    toast('删除失败: ' + fileErrMsg(e), 'error');
  }
  await loadFiles();
}

/* ---- 文件上传（按钮选择 / 拖拽均可，多文件逐个上传） ---- */
function uploadOne(serverId, rel, file, overwrite) {
  return api(serverPath(serverId, 'files/upload?path=' + encodeURIComponent(rel) +
    (overwrite ? '&overwrite=1' : '')), {
    method: 'POST',
    headers: { 'Content-Type': 'application/octet-stream' },
    body: file
  });
}

async function uploadFiles(fileList) {
  const serverId = currentServerId();
  if (!serverId) return;
  const basePath = filesPath;
  const files = Array.from(fileList || []);
  if (!files.length) return;
  // 单文件大小不设固定上限：由服务端按目标磁盘可用空间判断，
  // 空间不足时返回 507（message 会经 fileErrMsg 原样提示）
  let okCount = 0;
  for (let i = 0; i < files.length; i++) {
    const f = files[i];
    if (!f.name) continue;
    setInlineMsg('files-msg', 'warn',
      '正在上传 ' + (i + 1) + '/' + files.length + ': ' + f.name + ' (' + fmtSize(f.size) + ')…');
    const rel = joinRel(basePath, f.name);
    try {
      await uploadOne(serverId, rel, f, false);
      okCount++;
    } catch (e) {
      if (e.status === 409) {
        if (confirm('「' + f.name + '」已存在，覆盖吗？')) {
          try {
            await uploadOne(serverId, rel, f, true);
            okCount++;
          } catch (e2) {
            toast('上传失败: ' + f.name + ' - ' + fileErrMsg(e2), 'error');
          }
        }
      } else {
        toast('上传失败: ' + f.name + ' - ' + fileErrMsg(e), 'error');
      }
    }
  }
  setInlineMsg('files-msg', '', '');
  toast('上传完成: ' + okCount + '/' + files.length + ' 个文件', okCount === files.length ? 'success' : 'info');
  await loadFiles();
}

function showFileDropMask(show) {
  $('files-drop-mask').style.display = show ? 'flex' : 'none';
}
