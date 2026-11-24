/* ================= server.properties 配置 ================= */
function setInlineMsg(id, type, text) {
  const el = $(id);
  el.className = 'inline-msg ' + type;
  el.textContent = text;
}

async function loadProperties(force) {
  const serverId = currentServerId();
  if (!serverId) return false;
  if (!force && propsLoadedFor === serverId) return true;
  $('props-msg').className = 'inline-msg';
  let d;
  try {
    d = await api(serverPath(serverId, 'properties'));
  } catch (e) {
    if (currentServerId() === serverId) setInlineMsg('props-msg', 'err', '配置加载失败: ' + e.message);
    return false;
  }
  if (currentServerId() !== serverId) return false;
  propState = {
    exists: !!d.exists,
    entries: d.entries || {},
    order: d.order || Object.keys(d.entries || {}),
    dirty: new Set(),
    newKeys: new Set()
  };
  propsLoadedFor = serverId;
  renderProperties();
  return true;
}

function renderProperties() {
  const tbody = $('props-tbody');
  tbody.innerHTML = '';
  propInputs.clear();
  if (!propState) return;
  $('props-banner').className = propState.exists ? 'inline-msg' : 'inline-msg warn';
  if (!propState.exists) $('props-banner').textContent = 'server.properties 文件不存在，将在保存时创建';
  propState.order.forEach(k => addPropRow(k, propState.entries[k] != null ? propState.entries[k] : '', false));
  propState.newKeys.forEach(k => addPropRow(k, propInputs.has(k) ? propInputs.get(k).value : '', true));
}

function addPropRow(key, value, isNew) {
  if (propInputs.has(key)) return;
  const tr = document.createElement('tr');
  if (isNew) { tr.classList.add('new-row'); propState.dirty.add(key); }
  const tdK = document.createElement('td');
  tdK.className = 'prop-key';
  tdK.textContent = key;
  const tdV = document.createElement('td');
  const input = document.createElement('input');
  input.value = value;
  input.spellcheck = false;
  input.autocomplete = 'off';
  input.addEventListener('input', () => {
    const dirty = propState.newKeys.has(key) || input.value !== (propState.entries[key] != null ? propState.entries[key] : '');
    if (dirty) propState.dirty.add(key); else propState.dirty.delete(key);
    tr.classList.toggle('dirty', dirty && !propState.newKeys.has(key));
  });
  tdV.appendChild(input);
  tr.appendChild(tdK);
  tr.appendChild(tdV);
  $('props-tbody').appendChild(tr);
  propInputs.set(key, input);
}

function addNewProp() {
  if (!propState) return;
  const k = $('prop-new-key').value.trim();
  const v = $('prop-new-value').value;
  if (!k) { setInlineMsg('props-msg', 'err', '键名不能为空'); return; }
  if (/\s/.test(k)) { setInlineMsg('props-msg', 'err', '键名不能包含空白字符'); return; }
  if (propInputs.has(k) || Object.prototype.hasOwnProperty.call(propState.entries, k)) {
    setInlineMsg('props-msg', 'err', '键「' + k + '」已存在');
    return;
  }
  $('props-msg').className = 'inline-msg';
  propState.newKeys.add(k);
  addPropRow(k, v, true);
  $('prop-new-key').value = '';
  $('prop-new-value').value = '';
  setInlineMsg('props-msg', 'ok', '已添加新键「' + k + '」，点击「保存修改」后生效');
}

function setPropertiesBusy(busy) {
  $('props-save').disabled = busy;
  $('prop-add-btn').disabled = busy;
  $('prop-new-key').disabled = busy;
  $('prop-new-value').disabled = busy;
  propInputs.forEach(input => { input.disabled = busy; });
}

async function saveProperties() {
  const serverId = currentServerId();
  if (!serverId || !propState) return;
  if (propState.dirty.size === 0) {
    setInlineMsg('props-msg', 'warn', '没有需要保存的修改');
    return;
  }
  const state = propState;
  const entries = {};
  propState.dirty.forEach(k => { entries[k] = propInputs.get(k).value; });
  const n = propState.dirty.size;
  let busyState = state;
  // 保存后会从磁盘重载；暂时禁止编辑，避免重载覆盖请求期间的新输入。
  setPropertiesBusy(true);
  try {
    await api(serverPath(serverId, 'properties'), { method: 'PUT', body: JSON.stringify({ entries: entries }) });
    if (currentServerId() === serverId && propState === state) {
      propsLoadedFor = null;
      const loaded = await loadProperties(true);
      // 重载会清空旧提示；仅在原服务器重载成功后显示本次保存结果。
      if (loaded && currentServerId() === serverId) {
        busyState = propState;
        setInlineMsg('props-msg', 'ok', '保存成功，共写入 ' + n + ' 项');
        toast('配置已保存', 'success');
      }
    }
  } catch (e) {
    if (currentServerId() === serverId && propState === state) {
      setInlineMsg('props-msg', 'err', '保存失败: ' + e.message);
    }
  } finally {
    // 旧保存不能解除切换后新表单的 busy 状态。
    if (currentServerId() === serverId && propState === busyState) setPropertiesBusy(false);
  }
}
