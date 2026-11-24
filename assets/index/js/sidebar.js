/* ================= 侧栏 ================= */
function renderSidebar() {
  const list = $('server-list');
  const st = list.scrollTop;
  list.innerHTML = '';
  if (servers.length === 0) {
    const d = document.createElement('div');
    d.className = 'side-empty';
    d.textContent = '还没有服务器，点击上方「添加服务器」开始创建';
    list.appendChild(d);
    return;
  }
  servers.forEach((s, i) => {
    const item = document.createElement('div');
    item.className = 'srv-item' + (i === current ? ' active' : '');
    const line1 = document.createElement('div');
    line1.className = 'line1';
    const dot = document.createElement('span');
    dot.className = 'dot ' + (STATE_CLASS[s.state] || 'stopped');
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = s.name;
    line1.appendChild(dot);
    line1.appendChild(name);
    const line2 = document.createElement('div');
    line2.className = 'line2';
    line2.textContent = (s.core_name || '') + (s.mc_version ? ' · ' + s.mc_version : '');
    item.appendChild(line1);
    item.appendChild(line2);
    item.addEventListener('click', () => selectServer(i));
    list.appendChild(item);
  });
  list.scrollTop = st;
}

function closeSidebarMobile() {
  $('app').classList.remove('sidebar-open');
}
