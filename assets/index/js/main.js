/* ================= 主区 ================= */
function renderMain() {
  const has = current >= 0 && current < servers.length;
  $('empty-state').style.display = has ? 'none' : 'flex';
  $('server-view').style.display = has ? 'flex' : 'none';
  if (!has) { closeStream(); return; }
  renderHeader();
}

function badgeInner(state) {
  return '<span class="dot ' + (STATE_CLASS[state] || 'stopped') + '"></span>' +
         esc(STATE_TEXT[state] || state);
}

function renderHeader() {
  const s = servers[current];
  if (!s) return;
  $('srv-name').textContent = s.name;
  const meta = [s.core_name, s.mc_version ? 'MC ' + s.mc_version : '', s.core_version || '']
    .filter(Boolean).join(' · ');
  $('srv-meta').textContent = meta;
  const badge = $('srv-badge');
  badge.className = 'badge ' + (STATE_CLASS[s.state] || 'stopped');
  badge.innerHTML = badgeInner(s.state);
  updateActionButtons(s);
  updateCmdBar(s);
  // 状态相关的标签页内容随轮询刷新
  if (currentTab === 'overview') renderOverview();
  else if (currentTab === 'players') renderPlayers();
  else if (currentTab === 'backups') renderBackupsPanel();
}

function updateActionButtons(s) {
  // STOPPED 只能启动；RUNNING/STARTING 可停止/重启；STOPPING 全部禁用
  const canStart = s.state === 'STOPPED';
  const canStop = s.state === 'RUNNING' || s.state === 'STARTING';
  document.querySelectorAll('[data-action]').forEach(btn => {
    const a = btn.getAttribute('data-action');
    btn.disabled = a === 'start' ? !canStart : !canStop;
  });
}

function updateCmdBar(s) {
  const running = s.state === 'RUNNING';
  $('cmd-input').disabled = !running;
  $('cmd-send').disabled = !running;
  $('cmd-input').placeholder = running ? '输入服务器指令，回车发送' : '服务器未运行，无法发送指令';
}

// 切换选中的服务器时清理所有按服务器缓存的状态。
function resetServerScopedState() {
  closeStream();
  propsLoadedFor = null;
  propState = null;
  backupsFor = null;
  lastBackups = [];
  filesPath = '';
  filesEntries = [];
  fileEditing = null;
  ++fileReadSerial;
  fillServerForm();
}

async function selectServer(i) {
  if (i < 0 || i >= servers.length) return;
  const nextId = servers[i].id;
  const changed = nextId !== selectedServerId;
  current = i;
  selectedServerId = nextId;
  closeSidebarMobile();
  if (changed) resetServerScopedState();
  renderSidebar();
  renderMain();
  if (changed) await loadTabData(currentTab);
}

function switchTab(tab) {
  if (currentTab === tab) return;
  const prev = currentTab;
  currentTab = tab;
  document.querySelectorAll('.tab-btn').forEach(b =>
    b.classList.toggle('active', b.getAttribute('data-tab') === tab));
  TAB_NAMES.forEach(t => $('tab-' + t).classList.toggle('active', t === tab));
  if (prev === 'console' && tab !== 'console') closeStream();
  loadTabData(tab);
}

async function loadTabData(tab) {
  if (!currentServerId()) return;
  if (tab === 'console') await loadConsole();
  else if (tab === 'properties') await loadProperties(false);
  else if (tab === 'backups') await loadBackups();
  else if (tab === 'files') await loadFiles();
  else if (tab === 'players') renderPlayers();
  else if (tab === 'overview') renderOverview();
  else if (tab === 'settings') fillServerForm();
}
