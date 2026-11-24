/* ================= 事件绑定与初始化 ================= */
function bindAppShellEvents() {
  $('btn-add').addEventListener('click', openAddModal);
  $('btn-global-settings').addEventListener('click', openSettingsModal);
  $('btn-menu').addEventListener('click', () => $('app').classList.toggle('sidebar-open'));
  $('sidebar-mask').addEventListener('click', closeSidebarMobile);
}

function bindTabEvents() {
  document.querySelectorAll('.tab-btn').forEach(b =>
    b.addEventListener('click', () => switchTab(b.getAttribute('data-tab'))));
}

function bindActionEvents() {
  // 启动 / 停止 / 重启（顶栏与概览页共用，事件委托）
  document.addEventListener('click', e => {
    const btn = e.target.closest('[data-action]');
    if (btn && !btn.disabled) doAction(btn.getAttribute('data-action'));
  });
}

function bindConsoleEvents() {
  $('cmd-form').addEventListener('submit', e => {
    e.preventDefault();
    const cmd = $('cmd-input').value.trim();
    if (cmd) { sendCommand(cmd); $('cmd-input').value = ''; }
  });
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      if (consoleFlushFrame !== null) cancelAnimationFrame(consoleFlushFrame);
      consoleFlushFrame = null;
    } else {
      scheduleConsoleFlush();
    }
  });
  $('console-log').addEventListener('scroll', () => {
    const box = $('console-log');
    consoleStick = box.scrollHeight - box.scrollTop - box.clientHeight < 48;
    updateBackToBottom();
  }, { passive: true });
  $('back-to-bottom').addEventListener('click', () => {
    consoleStick = true;
    scrollConsoleToBottom();
    updateBackToBottom();
  });
}

function bindPlayersEvents() {
  $('players-refresh').addEventListener('click', () => refresh(true));
}

function bindPropertiesEvents() {
  $('props-save').addEventListener('click', saveProperties);
  $('prop-add-btn').addEventListener('click', addNewProp);
}

function bindFilesEvents() {
  $('files-up').addEventListener('click', navUp);
  $('files-refresh').addEventListener('click', loadFiles);
  $('files-new-file').addEventListener('click', () => openCreateModal(false));
  $('files-new-dir').addEventListener('click', () => openCreateModal(true));
  $('files-save').addEventListener('click', saveFile);
  $('files-close').addEventListener('click', closeEditor);
  $('file-modal-ok').addEventListener('click', confirmFileModal);
  $('file-modal-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') confirmFileModal();
  });
  // 文件上传：按钮选择 + 拖拽到文件页
  $('files-upload').addEventListener('click', () => $('files-upload-input').click());
  $('files-upload-input').addEventListener('change', () => {
    uploadFiles($('files-upload-input').files);
    $('files-upload-input').value = '';
  });
  const filesZone = $('tab-files');
  filesZone.addEventListener('dragenter', e => {
    e.preventDefault();
    fileDragDepth++;
    showFileDropMask(true);
  });
  filesZone.addEventListener('dragover', e => e.preventDefault());
  filesZone.addEventListener('dragleave', () => {
    if (--fileDragDepth <= 0) {
      fileDragDepth = 0;
      showFileDropMask(false);
    }
  });
  filesZone.addEventListener('drop', e => {
    e.preventDefault();
    fileDragDepth = 0;
    showFileDropMask(false);
    uploadFiles(e.dataTransfer.files);
  });
  // 添加目录向导打开时，避免拖到弹窗外导致浏览器直接打开本地文件
  document.addEventListener('dragover', e => {
    if ($('modal-add').style.display === 'flex') e.preventDefault();
  });
  document.addEventListener('drop', e => {
    if ($('modal-add').style.display === 'flex') e.preventDefault();
  });
}

function bindServerSettingsEvents() {
  $('sv-save').addEventListener('click', saveServerConfig);
  $('sv-move').addEventListener('click', () => changeServerLocation(true));
  $('sv-relocate').addEventListener('click', () => changeServerLocation(false));
  $('sv-remove').addEventListener('click', removeServerRegistration);
  $('sv-delete').addEventListener('click', deleteServer);
}

function bindGlobalSettingsEvents() {
  $('set-save').addEventListener('click', saveSettings);
}

function bindConfirmDialogEvents() {
  $('eula-agree').addEventListener('click', agreeEula);
  $('rcon-agree').addEventListener('click', agreeRcon);
  $('rcon-skip').addEventListener('click', declineRcon);
  $('installer-agree').addEventListener('click', agreeInstaller);
  $('installer-cancel').addEventListener('click', closeInstallerModal);
  $('installer-x').addEventListener('click', closeInstallerModal);
  $('add-x').addEventListener('click', closeAddModal);
}

function bindSharedModalEvents() {
  document.querySelectorAll('[data-close]').forEach(b =>
    b.addEventListener('click', () => closeModal(b.getAttribute('data-close'))));
  // 路径输入框的「浏览」按钮
  document.querySelectorAll('[data-browse]').forEach(b =>
    b.addEventListener('click', () => {
      const input = $(b.getAttribute('data-browse'));
      openBrowse(b.getAttribute('data-mode') || 'file',
                 input ? input.value : '', b.getAttribute('data-title'), p => {
        if (input) {
          input.value = p;
          input.dispatchEvent(new Event('input'));
        }
      });
    }));
  $('browse-up').addEventListener('click', () => { if (browseState) loadBrowse(browseState.parent); });
  $('browse-go').addEventListener('click', () => { if (browseState) loadBrowse($('browse-path').value.trim()); });
  $('browse-path').addEventListener('keydown', e => {
    if (e.key === 'Enter' && browseState) loadBrowse($('browse-path').value.trim());
  });
  $('browse-ok').addEventListener('click', confirmBrowse);
  // 弹窗只允许通过 叉号 / 取消按钮 / Esc 关闭；点击遮罩层不关闭，
  // 避免误点或「按住拖出窗口外松开」导致已填写内容丢失。
  document.addEventListener('keydown', e => {
    if (e.key !== 'Escape') return;
    if ($('modal-browse').style.display === 'flex') closeModal('modal-browse');
    else if ($('modal-add').style.display === 'flex') closeAddModal();
    else if ($('modal-settings').style.display === 'flex') closeModal('modal-settings');
    else if ($('modal-eula').style.display === 'flex') closeModal('modal-eula');
    else if ($('modal-rcon').style.display === 'flex') closeModal('modal-rcon');
    else if ($('modal-installer').style.display === 'flex') closeInstallerModal();
    else if ($('modal-file').style.display === 'flex') closeModal('modal-file');
  });
}

// 集中注册各领域的静态事件监听。
function bindStaticEvents() {
  bindAppShellEvents();
  bindTabEvents();
  bindActionEvents();
  bindConsoleEvents();
  bindPlayersEvents();
  bindPropertiesEvents();
  bindFilesEvents();
  bindServerSettingsEvents();
  bindGlobalSettingsEvents();
  bindConfirmDialogEvents();
  bindSharedModalEvents();
}

(function init() {
  bindStaticEvents();
  boot();
})();
