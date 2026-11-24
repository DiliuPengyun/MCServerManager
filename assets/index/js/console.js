/* ================= 控制台 ================= */
function logClass(line) {
  if (/\b(ERROR|FATAL|SEVERE)\b/.test(line) || /Exception|Error:/.test(line)) return ' log-err';
  if (/\bWARN(ING)?\b/.test(line)) return ' log-warn';
  return '';
}

async function loadConsole() {
  const serverId = currentServerId();
  if (!serverId) return;
  closeStream();
  resetConsoleView();
  const loadId = consoleLoadId;
  try {
    const data = await api(serverPath(serverId, 'console?tail=500'));
    if (loadId !== consoleLoadId || currentServerId() !== serverId || currentTab !== 'console') return;
    (data.lines || []).forEach(line => appendConsoleLine(line));
  } catch (e) { /* 历史读取失败时仍可接收实时日志 */ }
  if (loadId === consoleLoadId && currentServerId() === serverId && currentTab === 'console') openStream(serverId);
}

// 服务端对 SSE 连接数有上限（超限返回 503）。连续失败时回退到短轮询，
// 保证控制台仍能持续刷新；重新进入控制台页或切换服务器时才会再次尝试 SSE。
let consolePollTimer = null;
let consoleStreamErrors = 0;

function stopConsolePolling() {
  if (consolePollTimer) { clearInterval(consolePollTimer); consolePollTimer = null; }
}

function startConsolePolling(serverId) {
  if (consolePollTimer) return;
  const tick = async () => {
    if (currentServerId() !== serverId || currentTab !== 'console') { stopConsolePolling(); return; }
    try {
      const data = await api(serverPath(serverId, 'console?tail=500'));
      if (currentServerId() !== serverId || currentTab !== 'console') return;
      resetConsoleView();
      (data.lines || []).forEach(line => appendConsoleLine(line));
    } catch (e) { /* 轮询失败时保留当前内容 */ }
  };
  tick();
  consolePollTimer = setInterval(tick, 2000);
}

function openStream(serverId) {
  if (evtSource) evtSource.close();
  stopConsolePolling();
  consoleStreamErrors = 0;
  const stream = new EventSource(serverPath(serverId, 'console/stream'));
  evtSource = stream;
  stream.onopen = () => {
    consoleStreamErrors = 0;
    stopConsolePolling();
  };
  stream.onmessage = ev => {
    if (evtSource !== stream || currentServerId() !== serverId || currentTab !== 'console') return;
    try {
      const line = JSON.parse(ev.data).line;
      appendConsoleLine(line);
    } catch (e) { /* 忽略解析失败 */ }
  };
  stream.onerror = () => {
    if (evtSource !== stream) return;
    if (++consoleStreamErrors >= 3) {
      // 服务端实时流配额已满或暂时不可用：退回轮询，避免无限重连
      stream.close();
      if (evtSource === stream) evtSource = null;
      startConsolePolling(serverId);
    }
  };
}

function closeStream() {
  if (evtSource) { evtSource.close(); evtSource = null; }
  stopConsolePolling();
  consoleStreamErrors = 0;
  ++consoleLoadId;
  clearConsoleQueue();
}

// 环形队列只保留最新日志，入队不触碰 DOM；后台标签页也不强制刷屏。
// 每帧限制创建数量和耗时，避免突发日志在一次渲染里长期占用主线程。
const CONSOLE_MAX_LINES = 3000;
const CONSOLE_FRAME_LINES = 200;
const CONSOLE_FRAME_MS = 4;
const consoleQueue = new Array(CONSOLE_MAX_LINES);
let consoleQueueStart = 0;
let consoleQueueSize = 0;
let consoleFlushFrame = null;
let consoleLoadId = 0;

function clearConsoleQueue() {
  if (consoleFlushFrame !== null) cancelAnimationFrame(consoleFlushFrame);
  consoleFlushFrame = null;
  consoleQueue.fill(undefined);
  consoleQueueStart = 0;
  consoleQueueSize = 0;
}

function appendConsoleLine(line) {
  if (typeof line !== 'string') return;
  consoleQueue[(consoleQueueStart + consoleQueueSize) % CONSOLE_MAX_LINES] = line;
  if (consoleQueueSize === CONSOLE_MAX_LINES) {
    consoleQueueStart = (consoleQueueStart + 1) % CONSOLE_MAX_LINES;
  } else {
    ++consoleQueueSize;
  }
  scheduleConsoleFlush();
}

function scheduleConsoleFlush() {
  if (consoleFlushFrame !== null || !consoleQueueSize || document.hidden || currentTab !== 'console') return;
  consoleFlushFrame = requestAnimationFrame(flushConsoleQueue);
}

function flushConsoleQueue() {
  consoleFlushFrame = null;
  if (!consoleQueueSize || document.hidden || currentTab !== 'console') return;
  const box = $('console-log');
  if (!box) { clearConsoleQueue(); return; }
  const frag = document.createDocumentFragment();
  const started = performance.now();
  let count = 0;
  while (consoleQueueSize && count < CONSOLE_FRAME_LINES) {
    const line = consoleQueue[consoleQueueStart];
    consoleQueue[consoleQueueStart] = undefined;
    consoleQueueStart = (consoleQueueStart + 1) % CONSOLE_MAX_LINES;
    --consoleQueueSize;
    const div = document.createElement('div');
    div.className = 'log-line' + logClass(line);
    div.textContent = line;
    frag.appendChild(div);
    ++count;
    if (performance.now() - started >= CONSOLE_FRAME_MS) break;
  }

  const removeCount = Math.max(0, box.childElementCount + count - CONSOLE_MAX_LINES);
  // 先读取旧布局，再统一修改；裁掉顶部历史时保持正在阅读的内容位置。
  const oldTop = !consoleStick && removeCount ? box.scrollTop : 0;
  const removedHeight = !consoleStick && removeCount
    ? box.children[removeCount].offsetTop - box.firstElementChild.offsetTop : 0;
  for (let i = 0; i < removeCount; ++i) box.firstElementChild.remove();
  box.appendChild(frag);
  if (consoleStick) scrollConsoleToBottom();
  else if (removeCount) box.scrollTop = Math.max(0, oldTop - removedHeight);
  scheduleConsoleFlush();
}

// 启动/重启后开启新会话：清空旧日志视图（SSE 流保持连接，新日志继续推送）
function resetConsoleView() {
  ++consoleLoadId;
  clearConsoleQueue();
  const box = $('console-log');
  if (box) box.textContent = '';
  consoleStick = true;
  updateBackToBottom();
}

function scrollConsoleToBottom() {
  const box = $('console-log');
  box.scrollTop = box.scrollHeight;
}

function updateBackToBottom() {
  $('back-to-bottom').style.display = consoleStick ? 'none' : 'inline-flex';
}

async function sendCommand(cmd) {
  const serverId = currentServerId();
  if (!serverId || !cmd) return;
  try {
    await api(serverPath(serverId, 'command'), { method: 'POST', body: JSON.stringify({ command: cmd }) });
  } catch (e) {
    if (e.code === 'not_running' || (e.data && e.data.error === 'not_running')) toast('服务器未运行，指令未发送', 'warn');
    else toast('指令发送失败: ' + e.message, 'error');
  }
}
