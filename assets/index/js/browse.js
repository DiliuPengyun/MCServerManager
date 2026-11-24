/* ================= 路径浏览弹窗 ================= */
function openBrowse(mode, initialPath, title, onOk) {
  browseState = { mode, path: '', parent: '', selected: '', request: 0, onOk };
  $('browse-title').textContent = title || (mode === 'dir' ? '选择目录' : '选择文件');
  $('browse-ok').textContent = mode === 'dir' ? '选择当前目录' : '选择';
  // 初始目录：文件模式取输入路径的父目录，目录模式取其自身
  let start = (initialPath || '').trim();
  if (start && mode === 'file' && !start.endsWith('/')) {
    const i = start.lastIndexOf('/');
    start = i > 0 ? start.slice(0, i) : '/';
  }
  openModal('modal-browse');
  loadBrowse(start, true);
}

async function loadBrowse(path, isInitial) {
  if (!browseState) return;
  const state = browseState;
  const request = ++state.request;
  const msg = $('browse-msg');
  msg.className = 'inline-msg'; msg.textContent = '';
  try {
    const d = await api('/api/browse?path=' + encodeURIComponent(path || '') +
                        '&show_files=' + (state.mode === 'file' ? '1' : '0'));
    if (browseState !== state || state.request !== request) return;
    browseState.path = d.path;
    browseState.parent = d.parent;
    browseState.selected = '';
    $('browse-selected').textContent = '';
    $('browse-path').value = d.path;
    $('browse-up').disabled = (d.path === d.parent);
    const list = $('browse-list');
    if (!d.entries.length) {
      list.innerHTML = '<div class="browse-empty">（空目录）</div>';
    } else {
      list.innerHTML = d.entries.map(e =>
        '<div class="browse-item" data-name="' + esc(e.name) + '" data-dir="' + (e.is_dir ? 1 : 0) + '">' +
        '<span class="b-icon">' + (e.is_dir ? '📁' : '📄') + '</span>' +
        '<span class="b-name">' + esc(e.name) + '</span></div>').join('');
      list.querySelectorAll('.browse-item').forEach(el => {
        el.addEventListener('click', () => {
          if (el.getAttribute('data-dir') === '1') {
            loadBrowse(joinBrowsePath(browseState.path, el.getAttribute('data-name')));
          } else {
            list.querySelectorAll('.browse-item.selected').forEach(x => x.classList.remove('selected'));
            el.classList.add('selected');
            browseState.selected = el.getAttribute('data-name');
            $('browse-selected').textContent = '已选择：' + browseState.selected;
          }
        });
        el.addEventListener('dblclick', () => {
          if (el.getAttribute('data-dir') !== '1') {
            browseState.selected = el.getAttribute('data-name');
            confirmBrowse();
          }
        });
      });
    }
  } catch (e) {
    if (browseState !== state || state.request !== request) return;
    if (isInitial && path) { loadBrowse('', true); return; }  // 初始路径无效时回退到家目录
    msg.className = 'inline-msg err';
    msg.textContent = e.message || '目录加载失败';
  }
}

function joinBrowsePath(dir, name) {
  return dir.replace(/\/+$/, '') + '/' + name;
}

function confirmBrowse() {
  if (!browseState) return;
  let p = browseState.path;
  if (browseState.mode === 'file') {
    if (!browseState.selected) { toast('请先选择一个文件', 'warn'); return; }
    p = joinBrowsePath(p, browseState.selected);
  }
  const cb = browseState.onOk;
  closeModal('modal-browse');
  browseState = null;
  if (cb) cb(p);
}
