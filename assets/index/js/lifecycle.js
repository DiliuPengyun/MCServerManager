/* ================= 启停操作 / EULA / RCON ================= */
function actionToastText(action) {
  return action === 'start' ? '启动指令已发送'
    : action === 'stop' ? '停止指令已发送' : '重启指令已发送';
}

// EULA / RCON / 安装器都用点击时的 UUID 建上下文；
// 即使列表刷新或用户切换选中项，后续动作也不会作用到另一台服务器。
function openEulaConfirm(serverId) {
  eulaContext = { id: serverId, name: serverNameById(serverId) };
  $('eula-srv').textContent = eulaContext.name;
  openModal('modal-eula');
}

function openRconConfirm(serverId, action) {
  rconContext = { id: serverId, name: serverNameById(serverId) };
  rconPendingAction = action || 'start';
  $('rcon-srv').textContent = rconContext.name;
  openModal('modal-rcon');
}

function openInstallerConfirm(serverId, action) {
  installerContext = { id: serverId, name: serverNameById(serverId) };
  installerPendingAction = action || 'start';
  $('installer-srv').textContent = installerContext.name;
  resetInstallerModal();
  openModal('modal-installer');
}

// operation.error.code 是最终协议里唯一的前置条件来源。
function handleLifecycleRequirement(error, serverId, action) {
  const code = error && error.code;
  if (code === 'installer_required') {
    openInstallerConfirm(serverId, action);
    return true;
  }
  if (code === 'eula_required') {
    openEulaConfirm(serverId);
    return true;
  }
  if (code === 'rcon_required') {
    openRconConfirm(serverId, action);
    return true;
  }
  return false;
}

// 对固定 UUID 执行启停操作：提交 operation、等待最终状态，再处理前置条件。
async function runServerAction(serverId, action, allowNoRcon) {
  if (!serverId) return;
  let operation;
  try {
    const body = allowNoRcon ? { allow_no_rcon: true } : {};
    operation = await api(serverPath(serverId, action), { method: 'POST', body: JSON.stringify(body) });
  } catch (e) {
    if (handleLifecycleRequirement(e, serverId, action)) return;
    toast('操作失败: ' + e.message, 'error');
    return;
  }
  try {
    await waitOperation(operation);
  } catch (e) {
    if (handleLifecycleRequirement(e, serverId, action)) return;
    toast('操作失败: ' + e.message, 'error');
    return;
  }
  toast(actionToastText(action), 'success');
  if (action !== 'stop' && currentTab === 'console' && currentServerId() === serverId) {
    resetConsoleView();
  }
  setTimeout(refresh, 400);
}

async function doAction(action, allowNoRcon) {
  return runServerAction(currentServerId(), action, allowNoRcon);
}

async function agreeEula() {
  const context = eulaContext;
  if (!context || !context.id) return;
  const serverId = context.id;
  try {
    const operation = await api(serverPath(serverId, 'eula'), { method: 'POST', body: '{}' });
    await waitOperation(operation);
    toast('已同意 EULA，服务器启动中', 'success');
    closeModal('modal-eula');
    eulaContext = null;
    if (currentTab === 'console' && currentServerId() === serverId) {
      resetConsoleView();
    }
    setTimeout(refresh, 400);
  } catch (e) {
    // EULA 通过后紧接着引导 RCON 启用
    if (e.code === 'rcon_required') {
      closeModal('modal-eula');
      eulaContext = null;
      openRconConfirm(serverId, 'start');
      return;
    }
    toast('EULA 操作失败: ' + e.message, 'error');
  }
}

// 同意启用 RCON：写入配置并启动；若原操作是重启（服务器仍在运行），再补一次重启使 RCON 生效
async function agreeRcon() {
  const context = rconContext;
  if (!context || !context.id) return;
  const serverId = context.id;
  const action = rconPendingAction;
  try {
    const operation = await api(serverPath(serverId, 'rcon'), { method: 'POST', body: '{}' });
    await waitOperation(operation);
    // 原操作是重启时服务器仍在运行，随后会补一次重启，不能提示“启动中”。
    toast(action === 'restart' ? '已启用 RCON，正在重启服务器' : '已启用 RCON，服务器启动中', 'success');
    closeModal('modal-rcon');
    rconContext = null;
    if (currentTab === 'console' && currentServerId() === serverId) {
      resetConsoleView();
    }
    if (action === 'restart') {
      await runServerAction(serverId, 'restart', true);
      return;
    }
    setTimeout(refresh, 400);
  } catch (e) {
    toast('启用 RCON 失败: ' + e.message, 'error');
  }
}

// 拒绝启用 RCON：按原动作直接启动/重启，玩家页保持不可用
async function declineRcon() {
  const context = rconContext;
  if (!context || !context.id) return;
  const serverId = context.id;
  const action = rconPendingAction;
  closeModal('modal-rcon');
  rconContext = null;
  await runServerAction(serverId, action, true);
}
