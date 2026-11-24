/* ================= 玩家 ================= */
function renderPlayers() {
  const s = servers[current];
  const box = $('players-content');
  if (!s) { box.innerHTML = ''; return; }
  const rconOk = !!s.rcon;
  $('players-title').textContent = '在线玩家' + (s.state === 'RUNNING' && rconOk ? '（' + (s.players || []).length + '）' : '');
  box.innerHTML = '';
  if (s.state !== 'RUNNING') {
    box.innerHTML = '<div class="side-empty">服务器未运行，启动后可查看在线玩家</div>';
    return;
  }
  if (!rconOk) {
    box.innerHTML = '<div class="side-empty">玩家管理不可用：该服务器未启用 RCON。<br>下次启动时选择「启用 RCON 并启动」即可使用本页。</div>';
    return;
  }
  const players = s.players || [];
  if (players.length === 0) {
    box.innerHTML = '<div class="side-empty">暂无在线玩家</div>';
    return;
  }
  const grid = document.createElement('div');
  grid.className = 'player-grid';
  players.forEach(p => {
    const chip = document.createElement('span');
    chip.className = 'player-chip';
    const nm = document.createElement('span');
    nm.className = 'pname';
    nm.textContent = p;
    const kick = document.createElement('button');
    kick.className = 'btn';
    kick.textContent = '踢出';
    kick.addEventListener('click', () => sendCommand('kick ' + p));
    const ban = document.createElement('button');
    ban.className = 'btn btn-danger';
    ban.textContent = '封禁';
    ban.addEventListener('click', () => {
      if (confirm('确定封禁玩家「' + p + '」吗？')) sendCommand('ban ' + p);
    });
    chip.appendChild(nm);
    chip.appendChild(kick);
    chip.appendChild(ban);
    grid.appendChild(chip);
  });
  box.appendChild(grid);
}
