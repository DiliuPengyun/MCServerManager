/* ================= 概览 ================= */
function renderOverview() {
  const s = servers[current];
  if (!s) return;
  const items = [
    ['运行状态', '<span class="badge ' + (STATE_CLASS[s.state] || 'stopped') + '">' + badgeInner(s.state) + '</span>', false],
    ['核心类型', s.core_name || '-', false],
    ['MC 版本', s.mc_version || '-', false],
    ['核心版本', s.core_version || '-', false],
    ['最小内存', (s.min_memory_mb != null ? s.min_memory_mb : '-') + ' MB', false],
    ['最大内存', (s.max_memory_mb != null ? s.max_memory_mb : '-') + ' MB', false],
    ['Java 路径', s.java_path || '-', true],
    ['工作目录', s.working_dir || '-', true],
    ['JAR 路径', s.server_jar_path || '-', true],
    ['世界目录', s.world_dir || '-', true],
    ['备份目录', s.backup_dir || '-', true],
  ];
  let html = '<div class="card"><h3 class="card-title">服务器信息</h3><div class="info-grid">';
  items.forEach(it => {
    html += '<div class="info-item"><div class="k">' + esc(it[0]) + '</div>' +
            '<div class="v' + (it[2] ? ' mono' : '') + '">' +
            (it[0] === '运行状态' ? it[1] : esc(it[1])) + '</div></div>';
  });
  html += '</div></div>';
  html += '<div class="card"><h3 class="card-title">操作</h3><div class="ov-actions">' +
          '<button class="btn btn-primary btn-lg" data-action="start">启动服务器</button>' +
          '<button class="btn btn-danger btn-lg" data-action="stop">停止服务器</button>' +
          '<button class="btn btn-lg" data-action="restart">重启服务器</button>' +
          '</div></div>';
  $('overview-content').innerHTML = html;
  updateActionButtons(s);
}
