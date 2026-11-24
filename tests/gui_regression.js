'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');

const binary = path.resolve(process.argv[2] || 'target/debug/MCServerManager');
const output = path.resolve(process.argv[3] || 'gui-latest');
const playwright = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(error => error ? reject(error) : resolve(port));
    });
  });
}

async function waitFor(fn, timeout = 10000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { return await fn(); } catch (error) { last = error; }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw last || new Error('等待页面状态超时');
}

(async () => {
  fs.mkdirSync(output, { recursive: true });
  assert.deepEqual(fs.readdirSync(output), [], 'GUI 证据目录必须为空');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'mcsm-gui-review-'));
  const xdg = path.join(root, 'xdg');
  const configDir = path.join(xdg, 'MCServerManager');
  const home = path.join(root, 'home');
  const browseTarget = path.join(home, '浏览目标');
  const defaultServerDir = path.join(root, '默认新建目录');
  fs.mkdirSync(configDir, { recursive: true });
  fs.mkdirSync(browseTarget, { recursive: true });
  fs.mkdirSync(defaultServerDir, { recursive: true });
  const uploadA = path.join(root, '上传 甲.bin');
  const uploadB = path.join(root, '上传 乙.bin');
  fs.writeFileSync(uploadA, Buffer.from([0, 1, 2, 255]));
  fs.writeFileSync(uploadB, Buffer.from([4, 3, 2, 1]));
  const servers = ['图形测试服甲', '图形测试服乙'].map((name, index) => {
    const working = path.join(root, `实例 ${index}`);
    fs.mkdirSync(path.join(working, 'world'), { recursive: true });
    fs.writeFileSync(path.join(working, 'server.jar'), 'dummy');
    fs.writeFileSync(path.join(working, 'world', 'level.dat'), `世界 ${index}`);
    fs.writeFileSync(path.join(working, 'server.properties'), 'motd=初始内容\nenable-rcon=false\n');
    fs.writeFileSync(path.join(working, '说明.txt'), `文件 ${index}`);
    return {
      id: index === 0 ? '123e4567-e89b-42d3-a456-426614174000' : '123e4567-e89b-42d3-a456-426614174001',
      name,
      core_type: 'custom',
      working_dir: working,
      server_jar_path: 'server.jar',
      java_path: 'java',
      min_memory_mb: 1024,
      max_memory_mb: 2048,
      rcon_port: 0,
      rcon_password: '',
    };
  });
  const port = await freePort();
  fs.writeFileSync(path.join(configDir, 'config.json'), JSON.stringify({
    schema_version: 2,
    default_server_dir: defaultServerDir,
    global_java_path: 'java',
    active_server_id: servers[0].id,
    active_server_index: 0,
    web_port: port,
    servers,
  }));
  const webLog = fs.openSync(path.join(output, 'web.log'), 'w');
  const env = {
    ...process.env,
    XDG_CONFIG_HOME: xdg,
    XDG_DATA_HOME: path.join(root, 'data'),
    HOME: home,
    LC_ALL: 'C.UTF-8',
    RUST_LOG: 'warn',
  };
  const backend = spawn(binary, ['web', String(port), '--foreground'], {
    cwd: root, env, stdio: ['ignore', webLog, webLog],
  });
  let browser;
  const errors = [];
  const passes = [];
  const host = `http://127.0.0.1:${port}`;
  const result = { passes, errors, root };

  async function screenshot(page, name) {
    fs.writeFileSync(path.join(output, `${name}.txt`), await page.locator('body').innerText());
    await page.screenshot({ path: path.join(output, `${name}.png`), fullPage: true });
  }

  try {
    await waitFor(async () => {
      const response = await fetch(`${host}/api/servers`);
      assert.equal(response.status, 200);
      const body = await response.json();
      assert.equal(body.length, 2);
    });
    browser = await playwright.chromium.launch({ headless: true, args: ['--no-sandbox'] });
    const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page.on('pageerror', error => errors.push(`pageerror: ${error.message}`));
    page.on('console', message => {
      if (message.type() === 'error') errors.push(`console: ${message.text()}`);
    });
    page.on('response', response => {
      if (response.status() >= 400) errors.push(`http: ${response.status()} ${response.url()}`);
    });
    page.on('dialog', dialog => dialog.accept());
    await page.goto(host, { waitUntil: 'domcontentloaded' });
    await page.getByRole('heading', { name: '图形测试服甲', exact: true }).waitFor();
    await screenshot(page, '01-overview');
    assert.ok((await page.locator('body').innerText()).includes('已停止'));
    passes.push('概览与停止状态');

    await page.getByRole('button', { name: '配置', exact: true }).click();
    const motd = page.locator('#props-tbody tr').filter({ hasText: 'motd' }).locator('input');
    await motd.waitFor();
    await motd.fill('图形回归内容');
    await page.getByRole('button', { name: '保存修改', exact: true }).click();
    await waitFor(async () => assert.equal(await motd.inputValue(), '图形回归内容'));
    await page.getByText('保存成功，共写入 1 项', { exact: true }).waitFor();
    await screenshot(page, '02-properties-saved');
    passes.push('属性编辑保存与重载');

    await page.getByRole('button', { name: '文件', exact: true }).click();
    const readmeRow = page.locator('#files-tbody tr').filter({ hasText: '说明.txt' });
    await readmeRow.getByRole('button', { name: '编辑', exact: true }).click();
    const editor = page.locator('#files-text');
    await editor.waitFor();
    await editor.fill('通过图形界面保存\n第二行');
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByText('文件已保存', { exact: true }).waitFor();
    await page.getByRole('button', { name: '关闭', exact: true }).click();
    await page.locator('#files-list-wrap').waitFor({ state: 'visible' });
    await readmeRow.getByRole('button', { name: '编辑', exact: true }).click();
    await editor.waitFor({ state: 'visible' });
    await waitFor(async () => assert.equal(await editor.inputValue(), '通过图形界面保存\n第二行'));
    await screenshot(page, '03-file-editor-reopened');
    await page.getByRole('button', { name: '关闭', exact: true }).click();

    const chooserPromise = page.waitForEvent('filechooser');
    await page.getByRole('button', { name: '上传文件', exact: true }).click();
    const chooser = await chooserPromise;
    await chooser.setFiles([uploadA, uploadB]);
    await page.getByText('上传完成: 2/2 个文件', { exact: true }).waitFor();
    await page.locator('#files-tbody').getByText('上传 甲.bin', { exact: true }).waitFor();
    await page.locator('#files-tbody').getByText('上传 乙.bin', { exact: true }).waitFor();
    await screenshot(page, '04-files-uploaded');
    passes.push('文件编辑、重新读取与多文件上传');

    await page.getByRole('button', { name: '备份', exact: true }).click();
    await page.getByRole('button', { name: '创建备份', exact: true }).click();
    await page.locator('.backup-row .bname').first().waitFor();
    await screenshot(page, '05-backup-created');
    await page.getByRole('button', { name: '删除', exact: true }).click();
    await page.getByText('暂无备份', { exact: true }).waitFor();
    passes.push('备份创建与删除');

    await page.getByRole('button', { name: '⚙ 　全局设置', exact: true }).click();
    await page.locator('[data-browse="set-folder"]').click();
    await page.locator('#modal-browse').waitFor({ state: 'visible' });
    await page.locator('#browse-list .browse-empty').waitFor();
    await page.locator('#browse-path').fill(home);
    await page.locator('#browse-go').click();
    await page.locator('#browse-list').getByText('浏览目标', { exact: true }).click();
    await page.getByRole('button', { name: '选择当前目录', exact: true }).click();
    assert.equal(await page.locator('#set-folder').inputValue(), browseTarget);
    await screenshot(page, '06-path-browse-selected');
    await page.locator('#modal-settings [data-close="modal-settings"]').first().click();
    passes.push('可视化路径浏览与回填');

    await page.getByRole('button', { name: '＋ 添加服务器', exact: true }).click();
    await page.locator('.method-card[data-method="jar"]').click();
    await page.getByRole('button', { name: '下一步', exact: true }).click();
    await page.locator('#wz-name').fill('仅填写不安装');
    await page.locator('#wz-path').fill(path.join(root, '实例 0', 'server.jar'));
    await screenshot(page, '07-add-wizard-form');
    await page.locator('#add-x').click();
    await page.locator('#modal-add').waitFor({ state: 'hidden' });
    passes.push('添加向导弹窗与安全取消');

    await page.getByRole('button', { name: '控制台', exact: true }).click();
    await page.locator('[data-tab="console"].active').waitFor();
    await page.locator('#tab-console.active').waitFor();
    await page.locator('#cmd-input').waitFor({ state: 'visible' });
    assert.equal(await page.locator('#cmd-input').isDisabled(), true);
    // 等待导航边框的 150ms 过渡结束，截图不应保留上一个标签的高亮。
    await page.waitForTimeout(200);
    await screenshot(page, '08-console-stopped');
    passes.push('停止态控制台禁用输入');

    assert.equal(fs.existsSync(path.join(root, '实例 0', 'eula.txt')), false);
    assert.ok(fs.readFileSync(path.join(root, '实例 0', 'server.properties'), 'utf8').includes('enable-rcon=false'));
    assert.deepEqual(errors, []);
    result.cleaned = true;
    fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify(result, null, 2));
    fs.writeFileSync(path.join(output, 'errors.json'), JSON.stringify(errors, null, 2));
    console.log(`GUI ${passes.length}/${passes.length} 通过；证据：${output}`);
  } catch (error) {
    errors.push(error.stack || String(error));
    result.errors = errors;
    fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify(result, null, 2));
    fs.writeFileSync(path.join(output, 'errors.json'), JSON.stringify(errors, null, 2));
    if (browser) {
      const pages = browser.contexts().flatMap(context => context.pages());
      if (pages[0]) await screenshot(pages[0], '99-failure').catch(() => {});
    }
    throw error;
  } finally {
    if (browser) await browser.close();
    backend.kill('SIGTERM');
    fs.closeSync(webLog);
    fs.rmSync(root, { recursive: true, force: true });
  }
})().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
