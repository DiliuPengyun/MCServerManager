'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.join(__dirname, '..');

function loadPartial(name, context, exports) {
  const filename = path.join(root, 'assets/index/js', name);
  const source = fs.readFileSync(filename, 'utf8');
  vm.createContext(context);
  vm.runInContext(`${source}\n;globalThis.__test = { ${exports.join(', ')} };`, context, { filename });
  return context.__test;
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

// 模拟浏览器中的 UUID 辅助函数，避免测试直接依赖真实 DOM 状态。
function serverContext(servers, current) {
  const context = {
    servers,
    current,
    currentServerId: () => (context.servers[context.current] ? context.servers[context.current].id : null),
    serverPath: (id, suffix) => '/api/servers/' + encodeURIComponent(id) + (suffix ? '/' + suffix : ''),
  };
  return context;
}

function elementMap() {
  const elements = new Map();
  return {
    elements,
    element: id => {
      if (!elements.has(id)) {
        elements.set(id, {
          style: {}, className: '', textContent: '', value: '', innerHTML: '',
          disabled: false, scrollTop: 0, appendChild: () => {},
        });
      }
      return elements.get(id);
    },
  };
}

async function testServerPathHelpers() {
  const { serverPath, operationPath } = loadPartial('api.js', {}, ['serverPath', 'operationPath']);
  assert.equal(serverPath('a b'), '/api/servers/a%20b');
  assert.equal(serverPath('server-a', 'files?path=world%2Fa'), '/api/servers/server-a/files?path=world%2Fa');
  assert.equal(serverPath('server-a', '/console?tail=500'), '/api/servers/server-a/console?tail=500');
  assert.equal(operationPath('op 1'), '/api/operations/op%201');
  assert.throws(() => serverPath(''), /UUID/);
  assert.throws(() => operationPath(null), /operation ID/);
}

async function testApiErrorExposesCode() {
  const context = {
    fetch: async () => ({
      ok: false,
      status: 409,
      json: async () => ({ error: 'eula_required', message: '需要同意 EULA' }),
    }),
  };
  const { api } = loadPartial('api.js', context, ['api']);
  let error = null;
  try { await api('/api/demo'); } catch (e) { error = e; }
  assert.ok(error);
  assert.equal(error.code, 'eula_required');
  assert.equal(error.status, 409);
  assert.equal(error.message, '需要同意 EULA');
}

async function testCurrentServerIdUsesUuid() {
  const context = { servers: [{ id: 'uuid-a' }, { id: 'uuid-b' }], current: 1 };
  const { currentServerId } = loadPartial('core.js', context, ['currentServerId']);
  assert.equal(currentServerId(), 'uuid-b');
  context.current = -1;
  assert.equal(currentServerId(), null);
}

async function testTaskLogOffset() {
  const { appendTaskLogLines } = loadPartial('core.js', {}, ['appendTaskLogLines']);
  const task = { logShown: 0 };
  const lines = [];
  const append = line => lines.push(line);
  appendTaskLogLines(task, { log_offset: 3, log: ['三', '四'] }, append);
  appendTaskLogLines(task, { log_offset: 5, log: ['五', '六'] }, append);
  appendTaskLogLines(task, { log_offset: 4, log: ['四', '五'] }, append);
  assert.deepEqual(lines, ['三', '四', '五', '六']);
  assert.equal(task.logShown, 7);
}

async function testWaitOperationPropagatesErrorCode() {
  const responses = [
    { operation_id: 'op-1', status: 'error', error: { code: 'eula_required', message: '需要同意 EULA' } },
  ];
  const urls = [];
  const context = {
    api: async url => { urls.push(url); return responses.shift(); },
    operationPath: id => '/api/operations/' + encodeURIComponent(id),
    setTimeout: fn => { fn(); return 0; },
  };
  const { waitOperation } = loadPartial('core.js', context, ['waitOperation']);
  let error = null;
  try {
    await waitOperation({ operation_id: 'op-1', status: 'running' });
  } catch (e) { error = e; }
  assert.ok(error);
  assert.equal(error.code, 'eula_required');
  assert.equal(error.message, '需要同意 EULA');
  assert.deepEqual(urls, ['/api/operations/op-1']);
}

async function testWaitOperationStreamsOperationLog() {
  const responses = [
    { operation_id: 'op-2', status: 'running', log: ['三'], log_offset: 2 },
    {
      operation_id: 'op-2', status: 'done', log: ['四', '五'], log_offset: 3,
      result: { log: ['一', '二', '三', '四', '五'] },
    },
  ];
  const context = {
    api: async () => responses.shift(),
    operationPath: id => '/api/operations/' + encodeURIComponent(id),
    setTimeout: fn => { fn(); return 0; },
  };
  const { waitOperation, appendTaskLogLines } =
    loadPartial('core.js', context, ['waitOperation', 'appendTaskLogLines']);
  const task = { logShown: 0 };
  const lines = [];
  const result = await waitOperation(
    { operation_id: 'op-2', status: 'running', log: ['一', '二'], log_offset: 0 },
    { onUpdate: operation => appendTaskLogLines(task, operation, line => lines.push(line)) }
  );
  assert.deepEqual(lines, ['一', '二', '三', '四', '五']);
  assert.equal(task.logShown, 5);
  assert.deepEqual(result.log, ['一', '二', '三', '四', '五']);
}

async function testRefreshRestoresSelectionByUuid() {
  let resets = 0;
  const context = {
    servers: [{ id: 'server-a' }, { id: 'server-b' }],
    current: 0,
    selectedServerId: 'server-a',
    currentTab: 'overview',
    modalDepth: 0,
    api: async () => [{ id: 'server-b' }, { id: 'server-a' }],
    renderSidebar: () => {},
    renderMain: () => {},
    resetServerScopedState: () => { resets += 1; },
    loadTabData: async () => {},
  };
  const { refresh } = loadPartial('boot.js', context, ['refresh']);
  await refresh(true);
  assert.equal(context.current, 1);
  assert.equal(context.selectedServerId, 'server-a');
  assert.equal(context.servers[context.current].id, 'server-a');
  assert.equal(resets, 0);

  // 选中的 UUID 已不在列表中时回退到第一台，并清理旧服务器缓存。
  context.servers = [{ id: 'server-a' }, { id: 'server-b' }];
  context.current = 1;
  context.selectedServerId = 'server-a';
  context.api = async () => [{ id: 'server-b' }, { id: 'server-c' }];
  await refresh(true);
  assert.equal(context.current, 0);
  assert.equal(context.selectedServerId, 'server-b');
  assert.equal(context.servers[context.current].id, 'server-b');
  assert.equal(resets, 1);
}

async function testUploadKeepsOriginalContext() {
  const urls = [];
  const context = Object.assign(serverContext([{ id: 'server-a' }, { id: 'server-b' }], 0), {
    filesPath: 'world',
    confirm: () => true,
    setInlineMsg: () => {},
    toast: () => {},
    loadFiles: async () => {},
    api: async url => { urls.push(url); return { ok: true }; },
  });
  const { uploadFiles } = loadPartial('files.js', context, ['uploadFiles']);
  context.loadFiles = async () => {};
  context.renderFiles = () => {};
  const pending = uploadFiles([{ name: 'a.jar', size: 1 }, { name: 'b.jar', size: 1 }]);
  context.current = 1;
  context.filesPath = '其他目录';
  await pending;
  assert.deepEqual(urls, [
    '/api/servers/server-a/files/upload?path=world%2Fa.jar',
    '/api/servers/server-a/files/upload?path=world%2Fb.jar',
  ]);
}

async function testStaleFileReadIsIgnored() {
  const response = deferred();
  const context = Object.assign(serverContext([{ id: 'server-a' }, { id: 'server-b' }], 0), {
    filesPath: '',
    fileEditing: null,
    fileReadSerial: 0,
    api: () => response.promise,
    setInlineMsg: () => {},
    renderFiles: () => { throw new Error('旧读取不应触发绘制'); },
  });
  const { openFile } = loadPartial('files.js', context, ['openFile']);
  context.renderFiles = () => { throw new Error('旧读取不应触发绘制'); };
  const pending = openFile('旧.txt');
  context.current = 1;
  response.resolve({ content: '旧服务器内容' });
  await pending;
  assert.equal(context.fileEditing, null);
}

async function testStaleVersionsAreIgnored() {
  const paper = deferred(), forge = deferred();
  const context = {
    wiz: {
      step: 2, method: 'auto', core: 'paper', versions: [], versionsLoading: false,
      versionsRequest: 0,
    },
    renderWiz: () => {},
    toast: () => {},
    api: url => url.includes('core=paper') ? paper.promise : forge.promise,
  };
  const { loadWizVersions } = loadPartial('wizard.js', context, ['loadWizVersions']);
  context.renderWiz = () => {};
  const oldRequest = loadWizVersions('paper');
  context.wiz.core = 'forge';
  const newRequest = loadWizVersions('forge');
  forge.resolve({ versions: [{ id: 'forge-new' }] });
  await newRequest;
  paper.resolve({ versions: [{ id: 'paper-old' }] });
  await oldRequest;
  assert.equal(context.wiz.versions[0].id, 'forge-new');
  assert.equal(context.wiz.versionsLoading, false);
}

async function testStaleBrowseIsIgnored() {
  const oldResponse = deferred(), newResponse = deferred();
  const elements = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, { style: {}, className: '', textContent: '', value: '' });
    return elements.get(id);
  };
  const context = {
    browseState: { mode: 'dir', path: '', parent: '', selected: '', request: 0, onOk: null },
    $: element,
    api: url => url.includes(encodeURIComponent('/旧')) ? oldResponse.promise : newResponse.promise,
  };
  const { loadBrowse } = loadPartial('browse.js', context, ['loadBrowse']);
  const oldRequest = loadBrowse('/旧');
  const newRequest = loadBrowse('/新');
  newResponse.resolve({ path: '/新', parent: '/', entries: [] });
  await newRequest;
  oldResponse.resolve({ path: '/旧', parent: '/', entries: [] });
  await oldRequest;
  assert.equal(context.browseState.path, '/新');
}

function createPropertiesContext(api, servers) {
  const dom = elementMap();
  const context = Object.assign(serverContext(servers || [{ id: 'server-a' }], 0), {
    propsLoadedFor: 'server-a',
    propState: {
      exists: true, entries: { motd: '旧内容' }, order: ['motd'],
      dirty: new Set(['motd']), newKeys: new Set(),
    },
    propInputs: new Map([['motd', { value: '新内容', disabled: false }]]),
    $: dom.element,
    api,
    toast: () => {},
  });
  const { saveProperties } = loadPartial('properties.js', context, ['saveProperties']);
  context.renderProperties = () => {};
  return { context, elements: dom.elements, saveProperties };
}

async function testPropertiesSuccessSurvivesReload() {
  const { context, elements, saveProperties } = createPropertiesContext(async (_url, options) => {
    if (options && options.method === 'PUT') return { ok: true };
    return { exists: true, entries: { motd: '新内容' }, order: ['motd'] };
  });
  await saveProperties();
  assert.equal(context.propState.entries.motd, '新内容');
  assert.equal(elements.get('props-msg').className, 'inline-msg ok');
  assert.equal(elements.get('props-msg').textContent, '保存成功，共写入 1 项');
}

async function testPropertiesReloadFailureKeepsError() {
  const { elements, saveProperties } = createPropertiesContext(async (_url, options) => {
    if (options && options.method === 'PUT') return { ok: true };
    throw new Error('读取失败');
  });
  await saveProperties();
  assert.equal(elements.get('props-msg').className, 'inline-msg err');
  assert.equal(elements.get('props-msg').textContent, '配置加载失败: 读取失败');
}

async function testStalePropertiesSaveKeepsNewContextBusy() {
  const { context, saveProperties } = createPropertiesContext(
    async () => ({ ok: true }),
    [{ id: 'server-a' }, { id: 'server-b' }]
  );
  const pending = saveProperties();
  context.current = 1;
  context.propState = {
    exists: true, entries: { motd: '另一服务器' }, order: ['motd'],
    dirty: new Set(['motd']), newKeys: new Set(),
  };
  const newInput = { value: '另一服务器的新修改', disabled: true };
  context.propInputs = new Map([['motd', newInput]]);
  await pending;
  assert.equal(newInput.disabled, true);
}

async function testLifecycleUsesCapturedUuid() {
  const urls = [];
  const context = {
    servers: [{ id: 'server-a', name: 'A' }, { id: 'server-b', name: 'B' }],
    current: 1,
    currentServerId: () => 'server-b',
    serverPath: (id, suffix) => '/api/servers/' + id + '/' + suffix,
    eulaContext: { id: 'server-a', name: 'A' },
    api: async url => { urls.push(url); return { operation_id: 'op-1', status: 'running' }; },
    waitOperation: async () => ({}),
    toast: () => {},
    closeModal: () => {},
    currentTab: 'overview',
    resetConsoleView: () => {},
    refresh: () => {},
    setTimeout: fn => { fn(); return 0; },
  };
  const { agreeEula } = loadPartial('lifecycle.js', context, ['agreeEula']);
  await agreeEula();
  assert.deepEqual(urls, ['/api/servers/server-a/eula']);
  assert.equal(context.eulaContext, null);
}

async function testLifecycleDispatchesOperationRequirement() {
  const seen = [];
  const context = {
    currentServerId: () => 'server-b',
    serverPath: (id, suffix) => '/api/servers/' + id + '/' + suffix,
    api: async () => ({ operation_id: 'op-2', status: 'running' }),
    waitOperation: async () => {
      const error = new Error('需要用户选择是否启用 RCON');
      error.code = 'rcon_required';
      throw error;
    },
    toast: () => {},
    currentTab: 'overview',
    setTimeout: () => 0,
    refresh: () => {},
  };
  const { runServerAction } = loadPartial('lifecycle.js', context, ['runServerAction']);
  context.openRconConfirm = (id, action) => { seen.push({ id, action }); };
  await runServerAction('server-a', 'restart');
  assert.deepEqual(seen, [{ id: 'server-a', action: 'restart' }]);
}

async function testInstallerPollsOperationLogAndResumesUuid() {
  const operations = [
    { operation_id: 'op-3', status: 'running', log: ['a', 'b'], log_offset: 0 },
    { operation_id: 'op-3', status: 'running', log: ['c'], log_offset: 2 },
    {
      operation_id: 'op-3', status: 'done', log: ['d'], log_offset: 3,
      result: { log: ['a', 'b', 'c', 'd'] },
    },
  ];
  const urls = [];
  const lines = [];
  const resumed = [];
  const dom = elementMap();
  const element = id => {
    const value = dom.element(id);
    value.appendChild = child => { if (child && child.textContent) lines.push(child.textContent); };
    return value;
  };
  const context = {
    installerTask: {
      id: 'op-3', timer: 1, logShown: 0, running: true, generation: 0, polling: false,
    },
    installerContext: { id: 'server-a', name: 'A' },
    installerPendingAction: 'start',
    api: async url => { urls.push(url); return operations.shift(); },
    operationPath: id => '/api/operations/' + id,
    appendTaskLogLines: (task, data, append) => {
      const log = Array.isArray(data.log) ? data.log : [];
      const offset = Number.isSafeInteger(data.log_offset) && data.log_offset >= 0 ? data.log_offset : 0;
      const start = Math.max(task.logShown, offset);
      for (let i = start; i < offset + log.length; ++i) append(log[i - offset]);
      task.logShown = Math.max(task.logShown, offset + log.length);
    },
    $: element,
    document: { createElement: () => ({ textContent: '' }) },
    toast: () => {},
    closeModal: () => {},
    clearInterval: () => {},
    refresh: async () => {},
  };
  const { pollInstallerTask } = loadPartial('installer.js', context, ['pollInstallerTask']);
  context.runServerAction = async (id, action) => { resumed.push({ id, action }); };
  await pollInstallerTask();
  await pollInstallerTask();
  await pollInstallerTask();
  assert.deepEqual(urls, [
    '/api/operations/op-3', '/api/operations/op-3', '/api/operations/op-3',
  ]);
  assert.deepEqual(lines, ['a', 'b', 'c', 'd']);
  assert.equal(context.installerTask.logShown, 4);
  assert.deepEqual(resumed, [{ id: 'server-a', action: 'start' }]);
  assert.equal(context.installerContext, null);
}

async function testInstallerStopsWhenOperationIsReclaimed() {
  const dom = elementMap();
  const messages = [];
  const task = {
    id: 'op-missing', timer: 1, logShown: 0, running: true, generation: 0, polling: false,
  };
  const context = {
    installerTask: task,
    api: async () => {
      const error = new Error('操作不存在或已被回收');
      error.status = 404;
      error.code = 'operation_not_found';
      throw error;
    },
    operationPath: id => '/api/operations/' + id,
    $: dom.element,
    clearInterval: () => {},
    toast: message => messages.push(message),
  };
  const { pollInstallerTask } = loadPartial('installer.js', context, ['pollInstallerTask']);
  await pollInstallerTask();
  assert.equal(task.running, false);
  assert.equal(task.timer, null);
  assert.equal(messages.length, 1);
  assert.match(messages[0], /已被回收/);
  assert.equal(dom.elements.get('installer-cancel').textContent, '关闭');
}

(async () => {
  const tests = [
    testServerPathHelpers,
    testApiErrorExposesCode,
    testCurrentServerIdUsesUuid,
    testTaskLogOffset,
    testWaitOperationPropagatesErrorCode,
    testWaitOperationStreamsOperationLog,
    testRefreshRestoresSelectionByUuid,
    testUploadKeepsOriginalContext,
    testStaleFileReadIsIgnored,
    testStaleVersionsAreIgnored,
    testStaleBrowseIsIgnored,
    testPropertiesSuccessSurvivesReload,
    testPropertiesReloadFailureKeepsError,
    testStalePropertiesSaveKeepsNewContextBusy,
    testLifecycleUsesCapturedUuid,
    testLifecycleDispatchesOperationRequirement,
    testInstallerPollsOperationLogAndResumesUuid,
    testInstallerStopsWhenOperationIsReclaimed,
  ];
  for (const test of tests) await test();
  console.log(`前端状态测试 ${tests.length}/${tests.length} 通过`);
})().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
