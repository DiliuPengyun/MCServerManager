/* ================= 安装器（整合包未安装） ================= */
function resetInstallerModal() {
  if (installerTask && installerTask.timer) clearInterval(installerTask.timer);
  $('installer-log').style.display = 'none';
  $('installer-log').innerHTML = '';
  $('installer-hint').style.display = 'none';
  $('installer-agree').style.display = '';
  $('installer-cancel').style.display = '';
  $('installer-cancel').textContent = '取消';
  installerTask = null;
}

// 安装进行中禁止关闭（安装继续在后端执行，但弹窗轮询会中断）
function closeInstallerModal() {
  if (installerTask && installerTask.running) { toast('安装进行中，暂时无法关闭', 'warn'); return; }
  closeModal('modal-installer');
}

function appendInstallerLog(task, operation) {
  const box = $('installer-log');
  if (!box) return;
  appendTaskLogLines(task, operation, line => {
    const d = document.createElement('div');
    d.textContent = line;
    box.appendChild(d);
  });
  box.scrollTop = box.scrollHeight;
}

function failInstaller(task, message) {
  if (installerTask !== task) return;
  if (task.timer) clearInterval(task.timer);
  task.timer = null;
  task.running = false;
  toast('安装失败: ' + message, 'error');
  $('installer-hint').style.display = 'none';
  $('installer-cancel').style.display = '';
  $('installer-cancel').textContent = '关闭';
}

// 用户同意运行安装器：提交 operation 并轮询进度，完成后用固定 UUID 继续原动作
async function agreeInstaller() {
  const context = installerContext;
  if (!context || !context.id) return;
  const serverId = context.id;
  let operation;
  try {
    operation = await api(serverPath(serverId, 'install'), { method: 'POST', body: '{}' });
  } catch (e) {
    toast('启动安装器失败: ' + e.message, 'error');
    return;
  }
  installerTask = {
    id: operation.operation_id,
    timer: null,
    logShown: 0,
    running: true,
    generation: 0,
    polling: false
  };
  $('installer-log').style.display = 'block';
  $('installer-log').innerHTML = '';
  $('installer-hint').style.display = 'block';
  $('installer-agree').style.display = 'none';
  $('installer-cancel').style.display = 'none';
  appendInstallerLog(installerTask, operation);
  installerTask.timer = setInterval(pollInstallerTask, 1000);
  pollInstallerTask();
}

async function pollInstallerTask() {
  const task = installerTask;
  if (!task || task.id == null || !task.running || task.polling) return;
  task.polling = true;
  const generation = ++task.generation;
  let operation;
  try {
    operation = await api(operationPath(task.id));
  } catch (e) {
    // 网络故障下个周期重试；操作已被回收则必须停止，不能永久显示安装中。
    if (e.status === 404 || e.code === 'operation_not_found') {
      failInstaller(task, e.message || '操作不存在或已被回收，请重新查询服务器状态');
    }
    return;
  } finally {
    task.polling = false;
  }
  if (installerTask !== task || task.generation !== generation) return;
  appendInstallerLog(task, operation);
  if (operation.status === 'running') return;
  clearInterval(task.timer);
  task.timer = null;
  task.running = false;
  if (operation.status === 'done') {
    const context = installerContext;
    const serverId = context && context.id;
    const action = installerPendingAction || 'start';
    closeModal('modal-installer');
    installerContext = null;
    installerPendingAction = 'start';
    toast('服务端安装完成，继续启动流程', 'success');
    await refresh(true);
    // 安装完成后继续原动作；UUID 固定为点击安装器时的服务器。
    if (serverId) await runServerAction(serverId, action);
  } else {
    const detail = operation.error;
    failInstaller(task, (detail && detail.message) || '未知错误');
  }
}
