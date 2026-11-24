/* ================= 添加服务器向导 ================= */
const WIZ_METHODS = [
  { id: 'auto', title: '自动安装', desc: '从官方源查询版本并下载安装服务端核心（Vanilla / Paper / Forge / NeoForge / Fabric）' },
  { id: 'upload', title: '从本地上传', desc: '上传当前设备上的服务端文件夹' },
  { id: 'import', title: '使用已有目录', desc: '直接使用主机上已有的服务端文件夹' },
  { id: 'installer', title: '安装器 JAR', desc: '运行管理器主机上的安装器 JAR（如 Forge / NeoForge 安装器）' },
  { id: 'jar', title: '核心 JAR', desc: '使用管理器主机上已有的服务端 JAR 文件创建实例' },
];
const PATH_LABELS = { import: '服务器目录路径', installer: '安装器 JAR 路径', jar: '核心 JAR 路径' };

// 各方式后台任务的动作名：使用已有目录与核心 JAR 只注册或复制，不经过安装。
function wizActionName() {
  if (wiz.method === 'upload') return '上传';
  if (wiz.method === 'import' || wiz.method === 'jar') return '导入';
  return '安装';
}

function resetWiz() {
  if (wiz && wiz.taskTimer) clearInterval(wiz.taskTimer);
  wiz = {
    step: 1, method: null,
    coretypes: (wiz && wiz.coretypes) || null,
    core: '', version: '', versions: [], versionsLoading: false, versionsRequest: 0,
    path: '', name: '', java: 'java', min: 1024, max: 4096, jvm: '',
    uploadRoot: '', uploadFiles: [], uploadDirs: [], uploadBytes: 0, uploadScanning: false,
    uploadedFiles: 0, uploadedBytes: 0, uploadFinalizing: false,
    err: '', phase: 'edit', taskId: null, taskStatus: '', taskError: '',
    lines: [], logShown: 0, taskTimer: null, taskPollFailures: 0, taskGeneration: 0
  };
}

function openAddModal() {
  resetWiz();
  renderWiz();
  openModal('modal-add');
}

// 受保护关闭：后台任务进行中禁止关闭
function closeAddModal() {
  if (wiz && wiz.phase === 'running') { toast('任务进行中，暂时无法关闭', 'warn'); return; }
  closeWizForce();
}

function closeWizForce() {
  if (wiz && wiz.taskTimer) { clearInterval(wiz.taskTimer); wiz.taskTimer = null; }
  if (wiz && wiz.method === 'upload' && wiz.taskId != null && wiz.phase !== 'done') {
    api('/api/uploads/' + wiz.taskId, { method: 'DELETE' }).catch(() => {});
  }
  closeModal('modal-add');
}

function renderWiz() {
  const body = $('add-body');
  const foot = $('add-foot');
  let dots = '<div class="wiz-steps">';
  ['选择方式', '填写信息', wizActionName()].forEach((t, i) => {
    const n = i + 1;
    dots += '<div class="wiz-step-dot' + (n === wiz.step ? ' active' : n < wiz.step ? ' done' : '') + '">' + n + '　' + t + '</div>';
  });
  dots += '</div>';

  if (wiz.step === 1) {
    let h = dots + '<div class="method-grid">';
    WIZ_METHODS.forEach(m => {
      h += '<div class="method-card' + (wiz.method === m.id ? ' sel' : '') + '" data-method="' + m.id + '">' +
           '<h4>' + esc(m.title) + '</h4><p>' + esc(m.desc) + '</p></div>';
    });
    h += '</div>';
    body.innerHTML = h;
    body.querySelectorAll('.method-card').forEach(c =>
      c.addEventListener('click', () => { wiz.method = c.getAttribute('data-method'); renderWiz(); }));
    foot.innerHTML = '<span class="foot-note">选择一种添加方式</span>' +
      '<button class="btn btn-primary" id="wz-next"' + (wiz.method ? '' : ' disabled') + '>下一步</button>';
    $('wz-next').addEventListener('click', () => {
      wiz.step = 2;
      wiz.err = '';
      renderWiz();
      if (wiz.method === 'auto') ensureCoretypes();
    });
    return;
  }

  if (wiz.step === 2) { renderWizStep2(body, foot, dots); return; }

  // 第 3 步：任务进度
  body.innerHTML = dots + '<div class="wiz-log" id="wiz-log"></div><div class="wiz-status" id="wiz-status"></div>';
  const box = $('wiz-log');
  wiz.lines.forEach(l => {
    const d = document.createElement('div');
    d.textContent = l;
    box.appendChild(d);
  });
  box.scrollTop = box.scrollHeight;
  updateWizStatus();
  if (wiz.phase === 'running') {
    foot.innerHTML = '<span class="foot-note">' + (wiz.method === 'upload' ? '目录上传中' : wizActionName() + '进行中') + '，请勿关闭本窗口…</span>';
  } else if (wiz.phase === 'done') {
    foot.innerHTML = '<span class="foot-note">服务器「' + esc(wiz.name) + '」已就绪</span>' +
                     '<button class="btn btn-primary" id="wz-finish">完成</button>';
    $('wz-finish').addEventListener('click', closeWizForce);
  } else {
    const canRetryUpload = wiz.method === 'upload' && wiz.taskId != null && wiz.taskStatus !== 'error';
    foot.innerHTML = '<span class="foot-note">' + (wiz.method === 'upload' ? '上传失败' : wizActionName() + '失败') +
                     '，可' + (canRetryUpload ? '重试或关闭' : '关闭后修正信息重试') + '</span>' +
                     (canRetryUpload ? '<button class="btn btn-primary" id="wz-retry">重试上传</button>' : '') +
                     '<button class="btn" id="wz-close2">关闭</button>';
    if (canRetryUpload) $('wz-retry').addEventListener('click', startInstall);
    $('wz-close2').addEventListener('click', closeWizForce);
  }
}

function wizMethodFieldsHtml() {
  if (wiz.method === 'auto') {
    return '<label class="fld"><span>核心类型</span><select id="wz-core">' + coreOptionsHtml() + '</select></label>' +
           '<label class="fld"><span>游戏版本</span><select id="wz-version"' + (wiz.versionsLoading ? ' disabled' : '') + '>' +
           versionOptionsHtml() + '</select>' +
           (wiz.versionsLoading ? '<small>正在查询版本列表，可能需要几秒钟…</small>' : '') + '</label>';
  }
  if (wiz.method === 'upload') {
    let h = '<div class="wiz-upload-zone" id="wz-upload-zone">' +
            '<input type="file" id="wz-dir-input" webkitdirectory directory multiple style="display:none">' +
            '<button class="btn btn-primary" type="button" id="wz-dir-pick"' + (wiz.uploadScanning ? ' disabled' : '') + '>选择本地服务器目录</button>' +
            '<strong>' + (wiz.uploadScanning ? '正在读取目录，请稍候…' : '或把整个服务器目录拖到这里') + '</strong>' +
            '<span>将完整复制到管理器的默认服务器目录，包含 world、mods、libraries 等子目录</span>' +
            '<small>浏览器上传不会保留 Linux 文件权限、所有者和符号链接；目标目录已存在时请更换名称</small></div>';
    if (wiz.uploadRoot) {
      h += '<div class="wiz-upload-summary">已选择：<b>' + esc(wiz.uploadRoot) + '</b><br>' +
           wiz.uploadFiles.length + ' 个文件，共 ' + esc(fmtSize(wiz.uploadBytes)) + '</div>';
    }
    return h;
  }
  return '<label class="fld"><span>' + esc(PATH_LABELS[wiz.method]) + '</span>' +
         '<div class="fld-path">' +
         '<input id="wz-path" spellcheck="false" value="' + esc(wiz.path) + '" placeholder="' +
         (wiz.method === 'import' ? '/path/to/server 目录' : '/path/to/file.jar') + '">' +
         '<button class="btn" type="button" id="wz-path-browse">浏览</button></div></label>';
}

function wizCommonFieldsHtml() {
  let h = '<label class="fld"><span>服务器名称</span><input id="wz-name" spellcheck="false" value="' + esc(wiz.name) + '" placeholder="例如 survival-1.20.1"></label>';
  h += '<div class="fld-row">' +
       '<label class="fld"><span>最小内存 (MB)</span><input id="wz-min" type="number" min="0" value="' + esc(String(wiz.min)) + '"></label>' +
       '<label class="fld"><span>最大内存 (MB)</span><input id="wz-max" type="number" min="0" value="' + esc(String(wiz.max)) + '"></label>' +
       '</div>';
  h += '<label class="fld"><span>Java 路径</span><div class="fld-path">' +
       '<input id="wz-java" spellcheck="false" value="' + esc(wiz.java) + '" placeholder="java">' +
       '<button class="btn" type="button" id="wz-java-browse">浏览</button></div></label>';
  h += '<label class="fld"><span>JVM 附加参数</span><input id="wz-jvm" spellcheck="false" value="' + esc(wiz.jvm) + '" placeholder="可留空，例如 -XX:+UseG1GC"></label>';
  return h;
}

function renderWizStep2Body(dots) {
  let h = dots;
  if (wiz.err) h += '<div class="inline-msg err">' + esc(wiz.err) + '</div>';
  return h + wizMethodFieldsHtml() + wizCommonFieldsHtml();
}

function bindWizStep2Inputs() {
  const bind = (id, fn) => { const el = $(id); if (el) el.addEventListener('input', fn); };
  bind('wz-path', e => { wiz.path = e.target.value; });
  bind('wz-name', e => { wiz.name = e.target.value; });
  bind('wz-java', e => { wiz.java = e.target.value; });
  bind('wz-min', e => { wiz.min = e.target.value; });
  bind('wz-max', e => { wiz.max = e.target.value; });
  bind('wz-jvm', e => { wiz.jvm = e.target.value; });
  const wpBtn = $('wz-path-browse');
  if (wpBtn) wpBtn.addEventListener('click', () => {
    openBrowse(wiz.method === 'import' ? 'dir' : 'file', $('wz-path').value,
               PATH_LABELS[wiz.method], p => {
      $('wz-path').value = p;
      $('wz-path').dispatchEvent(new Event('input'));
    });
  });
  $('wz-java-browse').addEventListener('click', () => {
    openBrowse('file', $('wz-java').value, '选择 Java 可执行文件', p => {
      $('wz-java').value = p;
      $('wz-java').dispatchEvent(new Event('input'));
    });
  });
  const coreSel = $('wz-core');
  if (coreSel) coreSel.addEventListener('change', e => {
    wiz.core = e.target.value;
    wiz.version = '';
    wiz.versions = [];
    if (wiz.core) loadWizVersions(wiz.core); else renderWiz();
  });
  const verSel = $('wz-version');
  if (verSel) verSel.addEventListener('change', e => {
    wiz.version = e.target.value;
    if (!wiz.name.trim() && wiz.core && wiz.version) {
      wiz.name = wiz.core + '-' + wiz.version;
      renderWiz();
    }
  });
}

function renderWizStep2Foot(foot) {
  foot.innerHTML = '<button class="btn" id="wz-back">上一步</button>' +
                   '<button class="btn btn-primary" id="wz-install"' + (wiz.uploadScanning ? ' disabled' : '') + '>' +
                   (wiz.method === 'upload' ? '开始上传' : '开始' + wizActionName()) + '</button>';
  $('wz-back').addEventListener('click', () => { wiz.step = 1; wiz.err = ''; renderWiz(); });
  $('wz-install').addEventListener('click', startInstall);
}

function renderWizStep2(body, foot, dots) {
  body.innerHTML = renderWizStep2Body(dots);
  bindWizardUploadPicker();
  bindWizStep2Inputs();
  renderWizStep2Foot(foot);
}

function coreOptionsHtml() {
  if (!wiz.coretypes) return '<option value="">正在加载…</option>';
  let h = '<option value="">请选择核心类型</option>';
  wiz.coretypes.forEach(c => {
    const disabled = c.id === 'spigot' || c.id === 'custom';
    h += '<option value="' + esc(c.id) + '"' + (disabled ? ' disabled' : '') +
         (wiz.core === c.id ? ' selected' : '') + '>' +
         esc(c.name) + (disabled ? '（不支持自动安装）' : '') + '</option>';
  });
  return h;
}

function versionOptionsHtml() {
  if (wiz.versionsLoading) return '<option value="">正在加载版本…</option>';
  if (!wiz.core) return '<option value="">请先选择核心类型</option>';
  if (wiz.versions.length === 0) return '<option value="">未获取到可用版本</option>';
  let h = '<option value="">请选择版本</option>';
  wiz.versions.forEach(v => {
    h += '<option value="' + esc(v.id) + '"' + (wiz.version === v.id ? ' selected' : '') + '>' +
         esc(v.label || v.id) + '</option>';
  });
  return h;
}

async function ensureCoretypes() {
  if (wiz.coretypes) return;
  try {
    wiz.coretypes = await api('/api/coretypes');
  } catch (e) {
    wiz.coretypes = [];
    toast('核心类型加载失败: ' + e.message, 'error');
  }
  if (wiz && wiz.step === 2 && wiz.method === 'auto') renderWiz();
}

async function loadWizVersions(core) {
  const state = wiz;
  const request = ++state.versionsRequest;
  state.versionsLoading = true;
  state.versions = [];
  renderWiz();
  try {
    const d = await api('/api/versions?core=' + encodeURIComponent(core));
    if (wiz !== state || state.core !== core || state.versionsRequest !== request) return;
    state.versions = d.versions || [];
  } catch (e) {
    if (wiz !== state || state.core !== core || state.versionsRequest !== request) return;
    state.versions = [];
    toast('版本列表加载失败: ' + e.message, 'error');
  }
  state.versionsLoading = false;
  if (wiz.step === 2 && wiz.method === 'auto') renderWiz();
}

function normalizeUploadRel(path) {
  const parts = String(path || '').split('/');
  const clean = [];
  for (const part of parts) {
    if (!part || part === '.') continue;
    if (part === '..') throw new Error('目录中包含无效路径');
    clean.push(part);
  }
  if (!clean.length) throw new Error('目录中包含空路径');
  return clean.join('/');
}

function setWizardUpload(rootName, files, dirs) {
  const previousRoot = wiz.uploadRoot;
  files.sort((a, b) => a.rel.localeCompare(b.rel));
  wiz.uploadRoot = rootName;
  wiz.uploadFiles = files;
  wiz.uploadDirs = Array.from(new Set(dirs));
  wiz.uploadBytes = files.reduce((sum, item) => sum + item.file.size, 0);
  wiz.uploadedFiles = 0;
  wiz.uploadedBytes = 0;
  wiz.uploadFinalizing = false;
  if (!wiz.name.trim() || wiz.name === previousRoot) wiz.name = rootName;
  wiz.err = '';
  renderWiz();
}

function selectWizardInput(fileList) {
  const files = Array.from(fileList || []);
  if (!files.length) return;
  const firstPath = files[0].webkitRelativePath || '';
  if (!firstPath) {
    wiz.err = '当前浏览器无法读取目录内容，请换用桌面版 Chrome、Edge 或 Firefox，或尝试拖拽目录';
    renderWiz();
    return;
  }
  const rootName = firstPath.split('/')[0];
  const items = [];
  try {
    for (const file of files) {
      const parts = (file.webkitRelativePath || '').split('/');
      if (parts[0] !== rootName || parts.length < 2) throw new Error('请选择同一个服务器目录');
      items.push({ file: file, rel: normalizeUploadRel(parts.slice(1).join('/')) });
    }
  } catch (e) {
    wiz.err = e.message;
    renderWiz();
    return;
  }
  setWizardUpload(rootName, items, []);
}

function readEntryFile(entry) {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}

function readEntryBatch(reader) {
  return new Promise((resolve, reject) => reader.readEntries(resolve, reject));
}

async function collectWizardEntry(entry, prefix, files, dirs) {
  if (entry.isFile) {
    const file = await readEntryFile(entry);
    files.push({ file: file, rel: normalizeUploadRel(prefix) });
    return;
  }
  if (!entry.isDirectory) return;
  if (prefix) dirs.push(normalizeUploadRel(prefix));
  const reader = entry.createReader();
  while (true) {
    const entries = await readEntryBatch(reader);
    if (!entries.length) break;
    for (const child of entries) {
      await collectWizardEntry(child, prefix ? prefix + '/' + child.name : child.name, files, dirs);
    }
  }
}

async function selectWizardDropEntries(entries) {
  const state = wiz;
  if (entries.length !== 1 || !entries[0] || !entries[0].isDirectory) {
    wiz.err = '请只拖入一个顶层 MC 服务器目录，不要拖入多个目录或单独的文件';
    renderWiz();
    return;
  }
  state.uploadScanning = true;
  state.err = '';
  renderWiz();
  try {
    const files = [], dirs = [];
    await collectWizardEntry(entries[0], '', files, dirs);
    if (wiz !== state) return;
    state.uploadScanning = false;
    setWizardUpload(entries[0].name, files, dirs);
  } catch (e) {
    if (wiz !== state) return;
    state.uploadScanning = false;
    state.err = '读取拖拽目录失败: ' + (e.message || e);
    renderWiz();
  }
}

function bindWizardUploadPicker() {
  const zone = $('wz-upload-zone');
  if (!zone) return;
  const input = $('wz-dir-input');
  $('wz-dir-pick').addEventListener('click', () => input.click());
  input.addEventListener('change', () => {
    selectWizardInput(input.files);
    input.value = '';
  });
  let dragDepth = 0;
  zone.addEventListener('dragenter', e => {
    e.preventDefault();
    dragDepth++;
    zone.classList.add('dragover');
  });
  zone.addEventListener('dragover', e => {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
  });
  zone.addEventListener('dragleave', () => {
    if (--dragDepth <= 0) {
      dragDepth = 0;
      zone.classList.remove('dragover');
    }
  });
  zone.addEventListener('drop', e => {
    e.preventDefault();
    dragDepth = 0;
    zone.classList.remove('dragover');
    const entries = [];
    for (const item of Array.from(e.dataTransfer.items || [])) {
      if (item.kind !== 'file') continue;
      const entry = item.webkitGetAsEntry && item.webkitGetAsEntry();
      if (entry) entries.push(entry);
    }
    if (!entries.length) {
      wiz.err = '浏览器未能读取拖入内容，请改用「选择本地服务器目录」';
      renderWiz();
      return;
    }
    selectWizardDropEntries(entries);
  });
}

async function uploadWizardDirectory() {
  const state = wiz;
  const nonEmptyDirs = new Set();
  for (const item of state.uploadFiles) {
    const parts = item.rel.split('/');
    parts.pop();
    let dir = '';
    for (const part of parts) {
      dir = dir ? dir + '/' + part : part;
      nonEmptyDirs.add(dir);
    }
  }
  const emptyDirs = state.uploadDirs.filter(dir => !nonEmptyDirs.has(dir))
    .sort((a, b) => a.split('/').length - b.split('/').length);
  for (const dir of emptyDirs) {
    await api('/api/uploads/' + state.taskId + '/directory', {
      method: 'POST', body: JSON.stringify({ path: dir })
    });
  }

  const files = state.uploadFiles;
  let next = 0, failure = null;
  const worker = async () => {
    while (!failure) {
      const index = next++;
      if (index >= files.length) return;
      const item = files[index];
      try {
        await api('/api/uploads/' + state.taskId + '/file?path=' + encodeURIComponent(item.rel), {
          method: 'POST',
          headers: { 'Content-Type': 'application/octet-stream' },
          body: item.file
        });
        if (wiz !== state) throw new Error('上传向导已关闭');
        state.uploadedFiles++;
        state.uploadedBytes += item.file.size;
        updateWizStatus();
        if (state.uploadedFiles % 25 === 0 || state.uploadedFiles === files.length) {
          appendWizLogLine('已上传 ' + state.uploadedFiles + '/' + files.length + ' 个文件');
        }
      } catch (e) {
        if (!failure) failure = e;
      }
    }
  };
  await Promise.all([worker(), worker()]);
  if (failure) throw failure;
}

function readInstallInput() {
  return {
    name: wiz.name.trim(),
    minMem: parseInt(wiz.min, 10),
    maxMem: parseInt(wiz.max, 10)
  };
}

// 返回错误文案；空字符串表示校验通过。
function validateInstallInput(name, minMem, maxMem) {
  if (!name) return '请填写服务器名称';
  if (/[\/\\]/.test(name) || name.indexOf('..') >= 0) return '服务器名称不能包含 /、\\ 或 ..';
  if (wiz.method === 'upload' && name.startsWith('.')) return '上传服务器的名称不能以点号开头';
  if (isNaN(minMem) || isNaN(maxMem) || minMem < 0 || maxMem < 0) return '内存配置无效，请输入不小于 0 的数字';
  if (maxMem < minMem) return '最大内存不能小于最小内存';
  if (wiz.method === 'auto') {
    if (!wiz.core) return '请选择核心类型';
    if (!wiz.version) return '请选择游戏版本';
  } else if (wiz.method === 'upload') {
    if (wiz.uploadScanning) return '正在读取本地目录，请稍候';
    if (!wiz.uploadFiles.length) return '请先选择或拖入一个包含文件的 MC 服务器目录';
  } else if (!wiz.path.trim()) {
    return '请填写' + PATH_LABELS[wiz.method];
  }
  return '';
}

function buildInstallBody(name, minMem, maxMem) {
  const body = {
    method: wiz.method,
    name: name,
    java_path: wiz.java.trim() || 'java',
    min_memory_mb: minMem,
    max_memory_mb: maxMem,
    jvm_args: wiz.jvm
  };
  if (wiz.method === 'auto') {
    body.core_type = wiz.core;
    body.mc_version = wiz.version;
  } else if (wiz.method !== 'upload') {
    body.path = wiz.path.trim();
  }
  return body;
}

function resetInstallRunState() {
  if (wiz.taskTimer) { clearInterval(wiz.taskTimer); wiz.taskTimer = null; }
  ++wiz.taskGeneration;
  wiz.step = 3;
  wiz.phase = 'running';
  wiz.lines = [];
  wiz.logShown = 0;
  wiz.taskStatus = '';
  wiz.taskError = '';
  wiz.taskPollFailures = 0;
  if (wiz.method === 'upload') {
    wiz.uploadedFiles = 0;
    wiz.uploadedBytes = 0;
    wiz.uploadFinalizing = false;
  }
}

function failInstallValidation(msg) {
  wiz.err = msg;
  renderWiz();
}

// 检查可复用的暂存上传任务。返回 true 表示 startInstall 应提前结束
// （任务已失效、已完成、已失败或正在收尾等待）。
async function resumeUploadTask() {
  let existing;
  try {
    existing = await api('/api/tasks/' + wiz.taskId);
  } catch (e) {
    if (e.status !== 404) throw e;
    wiz.taskId = null;
    wiz.step = 2;
    wiz.phase = 'edit';
    wiz.err = '原上传任务已失效，请重新开始上传（本地目录选择已保留）';
    renderWiz();
    return true;
  }
  if (existing.status === 'done') {
    wiz.taskStatus = 'done';
    wiz.phase = 'done';
    renderWiz();
    refresh(true);
    return true;
  }
  if (existing.status === 'error') {
    wiz.taskStatus = 'error';
    wiz.taskError = existing.error || '上传任务失败';
    wiz.phase = 'error';
    renderWiz();
    return true;
  }
  if (existing.finalizing) {
    wiz.uploadFinalizing = true;
    appendWizLogLine('原任务已完成接收，继续等待核心检测与注册…');
    wiz.taskTimer = setInterval(pollWizTask, 1000);
    return true;
  }
  return false;
}

// 上传目录。返回 true 表示已提前结束（原任务仍在收尾）。
async function startUploadInstall(body) {
  if (wiz.taskId != null) {
    if (await resumeUploadTask()) return true;
  }
  if (wiz.taskId == null) {
    const r = await api('/api/uploads', { method: 'POST', body: JSON.stringify(body) });
    wiz.taskId = r.task_id;
    appendWizLogLine('上传任务 #' + wiz.taskId + ' 已创建');
    appendWizLogLine('目标目录：' + r.target_dir);
  } else {
    appendWizLogLine('继续使用暂存任务 #' + wiz.taskId + ' 重试上传');
  }
  await uploadWizardDirectory();
  wiz.uploadFinalizing = true;
  updateWizStatus();
  await api('/api/uploads/' + wiz.taskId + '/finish', { method: 'POST' });
  return false;
}

async function startNonUploadInstall(body) {
  const r = await api('/api/servers', { method: 'POST', body: JSON.stringify(body) });
  wiz.taskId = r.task_id;
}

function handleInstallError(e) {
  if (wiz.method === 'upload' && wiz.taskId != null) {
    wiz.phase = 'error';
    wiz.taskStatus = '';
    wiz.taskError = fileErrMsg(e);
    wiz.uploadFinalizing = false;
    renderWiz();
  } else {
    wiz.step = 2;
    wiz.phase = 'edit';
    wiz.err = '创建' + wizActionName() + '任务失败: ' + fileErrMsg(e);
    renderWiz();
  }
}

function startWizTaskPolling() {
  wiz.taskTimer = setInterval(pollWizTask, 1000);
}

async function startInstall() {
  const { name, minMem, maxMem } = readInstallInput();
  const err = validateInstallInput(name, minMem, maxMem);
  if (err) { failInstallValidation(err); return; }

  const body = buildInstallBody(name, minMem, maxMem);
  resetInstallRunState();
  renderWiz();
  try {
    if (wiz.method === 'upload') {
      if (await startUploadInstall(body)) return;
    } else {
      await startNonUploadInstall(body);
    }
  } catch (e) {
    handleInstallError(e);
    return;
  }
  appendWizLogLine((wiz.method === 'upload' ? '上传文件已接收' : wizActionName() + '任务已创建') +
                   '（#' + wiz.taskId + '），等待核心检测与注册…');
  startWizTaskPolling();
}

async function pollWizTask() {
  if (!wiz || wiz.taskId == null) return;
  const state = wiz, taskId = state.taskId;
  const generation = ++state.taskGeneration;
  let t;
  try {
    t = await api('/api/tasks/' + taskId);
  } catch (e) {
    if (wiz !== state || state.taskGeneration !== generation) return;
    if (state.method === 'upload') {
      state.taskPollFailures = (state.taskPollFailures || 0) + 1;
      if (state.taskPollFailures >= 5) {
        clearInterval(state.taskTimer);
        state.taskTimer = null;
        state.phase = 'error';
        state.taskStatus = '';
        state.taskError = '连续多次无法查询上传任务，请检查连接后重试';
        renderWiz();
      }
    }
    return;
  }
  if (wiz !== state || state.taskGeneration !== generation) return;
  state.taskPollFailures = 0;
  appendTaskLogLines(state, t, appendWizLogLine);
  if (t.status === 'running') return;
  clearInterval(state.taskTimer);
  state.taskTimer = null;
  state.taskStatus = t.status;
  state.taskError = t.error || '';
  state.phase = t.status === 'done' ? 'done' : 'error';
  renderWiz();
  if (state.phase === 'done') {
    toast(state.method === 'upload' ? '服务器目录上传完成' : '服务器' + wizActionName() + '完成', 'success');
    refresh(true);
  } else {
    toast((state.method === 'upload' ? '上传失败: ' : wizActionName() + '失败: ') + (state.taskError || '未知错误'), 'error');
  }
}

function appendWizLogLine(line) {
  wiz.lines.push(line);
  const box = $('wiz-log');
  if (!box) return;
  const div = document.createElement('div');
  div.textContent = line;
  box.appendChild(div);
  box.scrollTop = box.scrollHeight;
}

function updateWizStatus() {
  const el = $('wiz-status');
  if (!el) return;
  if (wiz.phase === 'running') {
    el.className = 'wiz-status';
    if (wiz.method === 'upload') {
      const text = wiz.uploadFinalizing
        ? '文件上传完成，正在识别核心并注册服务器…'
        : wiz.taskId == null
          ? '正在创建上传任务…'
          : '正在上传 ' + wiz.uploadedFiles + '/' + wiz.uploadFiles.length + ' 个文件（' +
            fmtSize(wiz.uploadedBytes) + ' / ' + fmtSize(wiz.uploadBytes) + '）';
      el.innerHTML = '<span class="spinner"></span><span>' + esc(text) + '</span>';
    } else {
      el.innerHTML = '<span class="spinner"></span><span>' + wizActionName() + '进行中…</span>';
    }
  } else if (wiz.phase === 'done') {
    el.className = 'wiz-status ok';
    el.textContent = wiz.method === 'upload' ? '✔ 上传完成，服务器已添加到列表' : '✔ ' + wizActionName() + '完成，服务器已添加到列表';
  } else if (wiz.phase === 'error') {
    el.className = 'wiz-status err';
    el.textContent = (wiz.method === 'upload' ? '✘ 上传失败: ' : '✘ ' + wizActionName() + '失败: ') + (wiz.taskError || '未知错误');
  }
}
