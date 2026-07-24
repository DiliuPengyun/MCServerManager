#pragma once

// 内嵌的 Web UI 单页（无外部依赖，原生 JS + SSE）
namespace mc_manager::web {

inline const char *kIndexHtml = R"HTML(<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>MCServerManager · Web 控制台</title>
<style>
:root {
  --bg: #16181d;
  --panel: #1e2128;
  --panel-2: #23262e;
  --panel-3: #282c35;
  --border: #2c303a;
  --text: #e4e7ec;
  --text-dim: #9aa0ab;
  --accent: #5b9bd5;
  --success: #4caf7d;
  --warning: #e5b567;
  --danger: #e06c75;
  --mono: "JetBrains Mono", "Cascadia Code", "SF Mono", Consolas, "Liberation Mono", "Courier New", monospace;
  --sans: "Segoe UI", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body { height: 100%; }
body {
  font-family: var(--sans);
  background: var(--bg);
  color: var(--text);
  font-size: 14px;
  overflow: hidden;
}
::-webkit-scrollbar { width: 8px; height: 8px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: rgba(255,255,255,.14); border-radius: 4px; }
::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,.24); }

h1, h2, h3 { font-weight: 600; }
.mono { font-family: var(--mono); }
.dim { color: var(--text-dim); }

/* ---------- 按钮 / 输入 ---------- */
.btn {
  display: inline-flex; align-items: center; justify-content: center; gap: 6px;
  background: var(--panel-3); color: var(--text);
  border: 1px solid var(--border); border-radius: 8px;
  padding: 7px 16px; font-size: 13px; font-family: var(--sans);
  cursor: pointer; transition: background .15s, border-color .15s, filter .15s, opacity .15s;
  white-space: nowrap; user-select: none;
}
.btn:hover { background: #2f333d; border-color: #3a3f4b; }
.btn:active { filter: brightness(.9); }
.btn:disabled { opacity: .38; cursor: not-allowed; filter: none; }
.btn:disabled:hover { background: var(--panel-3); border-color: var(--border); }
.btn-primary { background: var(--accent); border-color: var(--accent); color: #10131a; font-weight: 600; }
.btn-primary:hover { background: #6da9dd; border-color: #6da9dd; }
.btn-primary:disabled:hover { background: var(--accent); border-color: var(--accent); }
.btn-danger { background: rgba(224,108,117,.14); border-color: rgba(224,108,117,.45); color: var(--danger); }
.btn-danger:hover { background: rgba(224,108,117,.24); border-color: rgba(224,108,117,.7); }
.btn-danger:disabled:hover { background: rgba(224,108,117,.14); border-color: rgba(224,108,117,.45); }
.btn-ghost { background: transparent; border-color: transparent; color: var(--text-dim); }
.btn-ghost:hover { background: var(--panel-2); color: var(--text); border-color: var(--border); }
.btn.block { width: 100%; }
.btn-lg { padding: 11px 26px; font-size: 14px; border-radius: 9px; }
.icon-btn {
  display: none; align-items: center; justify-content: center;
  width: 34px; height: 34px; flex: none;
  background: var(--panel-2); color: var(--text);
  border: 1px solid var(--border); border-radius: 8px;
  font-size: 16px; cursor: pointer;
}

input, select {
  background: #14161b; color: var(--text);
  border: 1px solid var(--border); border-radius: 8px;
  padding: 8px 12px; font-size: 13px; font-family: var(--sans);
  outline: none; transition: border-color .15s, box-shadow .15s;
  width: 100%;
}
input:focus, select:focus { border-color: var(--accent); box-shadow: 0 0 0 3px rgba(91,155,213,.18); }
input:disabled, select:disabled { opacity: .45; cursor: not-allowed; }
input::placeholder { color: #5c6270; }
select { appearance: none; background-image: linear-gradient(45deg, transparent 50%, var(--text-dim) 50%), linear-gradient(135deg, var(--text-dim) 50%, transparent 50%); background-position: calc(100% - 16px) 50%, calc(100% - 11px) 50%; background-size: 5px 5px; background-repeat: no-repeat; padding-right: 30px; }
select option { background: var(--panel); }

/* ---------- 徽标 / 状态点 ---------- */
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; flex: none; background: #5a606c; }
.dot.running { background: var(--success); }
.dot.starting, .dot.stopping { background: var(--warning); }
.dot.stopped { background: #5a606c; }
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: .25; } }
.badge {
  display: inline-flex; align-items: center; gap: 7px;
  padding: 3px 11px; border-radius: 999px;
  font-size: 12px; line-height: 1.6;
  border: 1px solid var(--border); background: var(--panel-2); color: var(--text-dim);
  white-space: nowrap;
}
.badge.running { color: var(--success); border-color: rgba(76,175,125,.4); background: rgba(76,175,125,.08); }
.badge.stopped { color: var(--text-dim); }
.badge.starting, .badge.stopping { color: var(--warning); border-color: rgba(229,181,103,.4); background: rgba(229,181,103,.08); }
.badge.starting .dot, .badge.stopping .dot { animation: pulse 1.1s ease-in-out infinite; }

/* ---------- 登录页 ---------- */
#auth-view { height: 100vh; display: flex; align-items: center; justify-content: center; padding: 20px; }
.auth-card {
  width: 380px; max-width: 100%;
  background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
  padding: 36px 32px 30px; text-align: center;
  box-shadow: 0 18px 50px rgba(0,0,0,.45);
}
.logo {
  width: 46px; height: 46px; border-radius: 11px; flex: none;
  display: inline-flex; align-items: center; justify-content: center;
  background: linear-gradient(135deg, #3d6d9c, #5b9bd5);
  color: #fff; font-weight: 700; font-size: 17px; font-family: var(--mono);
  letter-spacing: .5px;
}
.auth-card h1 { font-size: 19px; margin: 16px 0 6px; letter-spacing: .5px; }
.auth-card .dim { font-size: 13px; margin-bottom: 22px; }
.auth-card input { text-align: center; margin-bottom: 12px; }
.auth-err { color: var(--danger); font-size: 12.5px; min-height: 18px; margin-top: 10px; }

/* ---------- 整体布局 ---------- */
#app { display: flex; height: 100vh; }
#sidebar {
  width: 248px; flex: none;
  background: var(--panel); border-right: 1px solid var(--border);
  display: flex; flex-direction: column;
}
.brand { display: flex; align-items: center; gap: 11px; padding: 16px 16px 14px; border-bottom: 1px solid var(--border); }
.brand .logo { width: 36px; height: 36px; font-size: 13px; border-radius: 9px; }
.brand-name { font-size: 14px; font-weight: 600; letter-spacing: .3px; }
.brand-sub { font-size: 11.5px; color: var(--text-dim); margin-top: 2px; }
.side-actions { padding: 12px 12px 8px; }
#server-list { flex: 1; overflow-y: auto; padding: 4px 0 10px; }
.side-empty { padding: 26px 18px; color: var(--text-dim); font-size: 12.5px; text-align: center; line-height: 1.8; }
.srv-item {
  margin: 2px 8px; padding: 9px 12px;
  border-radius: 8px; border: 1px solid transparent;
  cursor: pointer; transition: background .15s, border-color .15s;
}
.srv-item:hover { background: var(--panel-2); }
.srv-item.active { background: var(--panel-2); border-color: var(--border); box-shadow: inset 2px 0 0 var(--accent); }
.srv-item .line1 { display: flex; align-items: center; gap: 8px; font-size: 13.5px; }
.srv-item .line1 .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.srv-item .line2 { font-size: 11.5px; color: var(--text-dim); margin-top: 4px; padding-left: 16px; }
.sidebar-footer { padding: 10px 12px; border-top: 1px solid var(--border); }
#sidebar-mask { display: none; }

#main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
#empty-state { flex: 1; display: flex; align-items: center; justify-content: center; padding: 24px; }
.empty-inner { text-align: center; color: var(--text-dim); }
.empty-inner .glyph {
  width: 64px; height: 64px; margin: 0 auto 18px;
  border: 1px solid var(--border); border-radius: 14px;
  background: var(--panel);
  display: flex; align-items: center; justify-content: center;
  font-family: var(--mono); font-size: 22px; color: var(--accent);
}
.empty-inner h2 { color: var(--text); font-size: 17px; margin-bottom: 8px; }
.empty-inner p { font-size: 13px; line-height: 1.8; }

#server-view { flex: 1; min-height: 0; display: none; flex-direction: column; }
#topbar {
  display: flex; align-items: center; gap: 14px; flex-wrap: wrap;
  padding: 13px 18px; border-bottom: 1px solid var(--border);
  background: var(--panel);
}
.srv-title-wrap { min-width: 0; flex: 1; }
.srv-title-line { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.srv-title-line h1 { font-size: 17px; letter-spacing: .3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.srv-meta { font-size: 12px; color: var(--text-dim); margin-top: 3px; }
.top-actions { display: flex; gap: 8px; }

#tabs {
  display: flex; gap: 2px; padding: 0 14px;
  border-bottom: 1px solid var(--border);
  background: var(--panel); overflow-x: auto; flex: none;
}
.tab-btn {
  background: none; border: none; border-bottom: 2px solid transparent;
  color: var(--text-dim); font-size: 13.5px; font-family: var(--sans);
  padding: 11px 15px 9px; cursor: pointer;
  transition: color .15s, border-color .15s;
  white-space: nowrap;
}
.tab-btn:hover { color: var(--text); }
.tab-btn.active { color: #fff; border-bottom-color: var(--accent); }

.panel { flex: 1; min-height: 0; display: none; }
.panel.active { display: flex; flex-direction: column; }
.panel-scroll { flex: 1; overflow-y: auto; padding: 18px; }
.panel-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; flex-wrap: wrap; }
.panel-head h3 { font-size: 15px; }

/* ---------- 卡片 / 信息栅格 ---------- */
.card {
  background: var(--panel); border: 1px solid var(--border); border-radius: 8px;
  padding: 18px; margin-bottom: 16px;
  box-shadow: 0 2px 10px rgba(0,0,0,.18);
}
.card-title { font-size: 14px; margin-bottom: 14px; padding-bottom: 10px; border-bottom: 1px solid var(--border); }
.info-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 10px; }
.info-item { background: var(--panel-2); border: 1px solid var(--border); border-radius: 8px; padding: 10px 13px; min-width: 0; }
.info-item .k { font-size: 11.5px; color: var(--text-dim); margin-bottom: 5px; }
.info-item .v { font-size: 13px; word-break: break-all; line-height: 1.5; }
.info-item .v.mono { font-family: var(--mono); font-size: 12px; }
.ov-actions { display: flex; gap: 12px; flex-wrap: wrap; }

/* ---------- 控制台 ---------- */
#console-wrap {
  flex: 1; min-height: 0; position: relative;
  margin: 14px 14px 0;
  border: 1px solid var(--border); border-radius: 8px; overflow: hidden;
  background: #0d0f12;
}
#console-log {
  position: absolute; inset: 0; overflow-y: auto;
  padding: 12px 14px;
  font-family: var(--mono); font-size: 12.5px; line-height: 1.5;
  white-space: pre-wrap; word-break: break-all;
  color: #c8cdd5;
}
.log-line.log-err { color: var(--danger); }
.log-line.log-warn { color: var(--warning); }
#back-to-bottom {
  position: absolute; right: 16px; bottom: 14px; z-index: 5;
  background: var(--accent); color: #10131a; font-weight: 600;
  border: none; border-radius: 999px; padding: 7px 16px; font-size: 12.5px;
  cursor: pointer; box-shadow: 0 4px 14px rgba(0,0,0,.4);
  transition: filter .15s;
}
#back-to-bottom:hover { filter: brightness(1.1); }
#cmd-form {
  display: flex; gap: 8px; padding: 12px 14px;
  border-top: 1px solid var(--border); background: var(--panel);
}
#cmd-form input { font-family: var(--mono); }

/* ---------- 玩家 ---------- */
.player-grid { display: flex; flex-wrap: wrap; gap: 10px; }
.player-chip {
  display: inline-flex; align-items: center; gap: 9px;
  background: var(--panel-2); border: 1px solid var(--border); border-radius: 999px;
  padding: 6px 8px 6px 14px; font-size: 13px;
}
.player-chip .pname { font-family: var(--mono); font-size: 12.5px; }
.player-chip .btn { padding: 3px 11px; font-size: 12px; border-radius: 999px; }

/* ---------- 配置表格 ---------- */
.table-wrap { border: 1px solid var(--border); border-radius: 8px; overflow: hidden; margin-bottom: 14px; }
.kv-table { width: 100%; border-collapse: collapse; }
.kv-table td { padding: 7px 12px; border-bottom: 1px solid var(--border); vertical-align: middle; }
.kv-table tr:last-child td { border-bottom: none; }
.kv-table tr.dirty { background: rgba(91,155,213,.06); }
.kv-table tr.new-row { background: rgba(76,175,125,.06); }
.kv-table .prop-key { font-family: var(--mono); font-size: 12.5px; color: var(--text-dim); width: 38%; word-break: break-all; }
.kv-table tr.dirty .prop-key, .kv-table tr.new-row .prop-key { color: var(--text); }
.kv-table input { padding: 6px 10px; font-size: 12.5px; font-family: var(--mono); background: transparent; border-color: transparent; }
.kv-table input:hover { border-color: var(--border); }
.kv-table input:focus { border-color: var(--accent); background: #14161b; }
.props-add { display: flex; gap: 8px; flex-wrap: wrap; }
.props-add input { flex: 1; min-width: 160px; }
.props-add .btn { flex: none; }

/* ---------- 备份 ---------- */
.backup-row {
  display: flex; align-items: center; gap: 12px;
  background: var(--panel-2); border: 1px solid var(--border); border-radius: 8px;
  padding: 10px 14px; margin-bottom: 8px;
}
.backup-row .bname { flex: 1; font-family: var(--mono); font-size: 12.5px; word-break: break-all; }
.backup-row .btn { padding: 4px 13px; font-size: 12px; }

/* ---------- 表单 ---------- */
.form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 13px 16px; }
.fld { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.fld > span { font-size: 12.5px; color: var(--text-dim); }
.fld small { font-size: 11.5px; color: #6b7280; line-height: 1.5; }
.fld.span2 { grid-column: span 2; }
.fld-row { display: grid; grid-template-columns: 1fr 1fr; gap: 13px 16px; }
.form-actions { display: flex; align-items: center; justify-content: flex-end; gap: 12px; margin-top: 16px; }
.inline-msg { font-size: 12.5px; border-radius: 7px; padding: 8px 12px; margin-bottom: 12px; display: none; }
.inline-msg.ok { display: block; color: var(--success); background: rgba(76,175,125,.1); border: 1px solid rgba(76,175,125,.35); }
.inline-msg.err { display: block; color: var(--danger); background: rgba(224,108,117,.1); border: 1px solid rgba(224,108,117,.35); }
.inline-msg.warn { display: block; color: var(--warning); background: rgba(229,181,103,.1); border: 1px solid rgba(229,181,103,.35); }
.danger-zone { border-color: rgba(224,108,117,.4); }
.danger-zone .card-title { color: var(--danger); border-bottom-color: rgba(224,108,117,.25); }
.dz-desc { font-size: 13px; color: var(--text-dim); line-height: 1.7; margin-bottom: 14px; }
.dz-desc b { color: var(--danger); }

/* ---------- 弹窗 ---------- */
.modal-mask {
  position: fixed; inset: 0; z-index: 100;
  background: rgba(0,0,0,.55);
  backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px);
  display: flex; align-items: center; justify-content: center; padding: 20px;
}
@keyframes modal-in { from { opacity: 0; transform: translateY(10px) scale(.98); } to { opacity: 1; transform: none; } }
.modal {
  width: 520px; max-width: 100%; max-height: 88vh;
  background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
  display: flex; flex-direction: column;
  box-shadow: 0 24px 70px rgba(0,0,0,.55);
  animation: modal-in .18s ease-out;
}
.modal-lg { width: 640px; }
.modal-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 15px 20px; border-bottom: 1px solid var(--border); flex: none;
}
.modal-head h3 { font-size: 15px; }
.modal-x {
  background: none; border: none; color: var(--text-dim);
  font-size: 21px; line-height: 1; cursor: pointer; padding: 2px 6px;
  border-radius: 6px; transition: color .15s, background .15s;
}
.modal-x:hover { color: var(--text); background: var(--panel-2); }
.modal-body { padding: 18px 20px; overflow-y: auto; }
.modal-body .fld { margin-bottom: 13px; }
.modal-body .fld-row { margin-bottom: 13px; }
.modal-foot {
  display: flex; align-items: center; justify-content: flex-end; gap: 10px;
  padding: 14px 20px; border-top: 1px solid var(--border); flex: none;
}
.modal-foot .foot-note { flex: 1; font-size: 12.5px; color: var(--text-dim); }
.eula-link { margin: 12px 0; font-size: 13px; }
.eula-link .mono { color: var(--accent); font-size: 12.5px; }
.modal-body p { font-size: 13.5px; line-height: 1.8; }

/* ---------- 向导 ---------- */
.wiz-steps { display: flex; gap: 6px; margin-bottom: 18px; }
.wiz-step-dot {
  flex: 1; text-align: center; font-size: 11.5px; color: var(--text-dim);
  padding: 7px 4px; border-radius: 7px; background: var(--panel-2);
  border: 1px solid var(--border); transition: all .15s;
}
.wiz-step-dot.active { color: var(--accent); border-color: rgba(91,155,213,.5); background: rgba(91,155,213,.08); }
.wiz-step-dot.done { color: var(--success); }
.method-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
.method-card {
  background: var(--panel-2); border: 1px solid var(--border); border-radius: 9px;
  padding: 14px 15px; cursor: pointer; transition: border-color .15s, background .15s;
}
.method-card:hover { border-color: #3a3f4b; background: var(--panel-3); }
.method-card.sel { border-color: var(--accent); background: rgba(91,155,213,.08); box-shadow: 0 0 0 3px rgba(91,155,213,.12); }
.method-card h4 { font-size: 13.5px; margin-bottom: 6px; }
.method-card p { font-size: 12px; color: var(--text-dim); line-height: 1.6; }
.wiz-log {
  background: #0d0f12; border: 1px solid var(--border); border-radius: 8px;
  height: 260px; overflow-y: auto; padding: 10px 12px;
  font-family: var(--mono); font-size: 12px; line-height: 1.55;
  white-space: pre-wrap; word-break: break-all; color: #c8cdd5;
}
.wiz-status { display: flex; align-items: center; gap: 9px; margin-top: 13px; font-size: 13px; }
@keyframes spin { to { transform: rotate(360deg); } }
.spinner {
  width: 14px; height: 14px; flex: none;
  border: 2px solid rgba(91,155,213,.3); border-top-color: var(--accent);
  border-radius: 50%; animation: spin .8s linear infinite;
}
.wiz-status.ok { color: var(--success); }
.wiz-status.err { color: var(--danger); }

/* ---------- Toast ---------- */
#toast-root {
  position: fixed; top: 16px; left: 50%; transform: translateX(-50%);
  z-index: 300; display: flex; flex-direction: column; gap: 8px;
  align-items: center; pointer-events: none; max-width: min(92vw, 560px);
}
.toast {
  background: var(--panel-3); border: 1px solid var(--border);
  border-left: 3px solid var(--accent);
  color: var(--text); font-size: 13px;
  border-radius: 8px; padding: 9px 16px;
  box-shadow: 0 8px 26px rgba(0,0,0,.45);
  opacity: 0; transform: translateY(-8px);
  transition: opacity .2s, transform .2s;
  max-width: 100%; word-break: break-all;
}
.toast.show { opacity: 1; transform: none; }
.toast.success { border-left-color: var(--success); }
.toast.error { border-left-color: var(--danger); }
.toast.warn { border-left-color: var(--warning); }

/* ---------- 响应式 ---------- */
@media (max-width: 860px) {
  #sidebar {
    position: fixed; left: 0; top: 0; bottom: 0; z-index: 80;
    transform: translateX(-105%); transition: transform .2s ease;
    box-shadow: 0 0 46px rgba(0,0,0,.55);
  }
  #app.sidebar-open #sidebar { transform: none; }
  #sidebar-mask {
    display: none; position: fixed; inset: 0; z-index: 70;
    background: rgba(0,0,0,.45);
  }
  #app.sidebar-open #sidebar-mask { display: block; }
  .icon-btn { display: inline-flex; }
  .form-grid { grid-template-columns: 1fr; }
  .fld.span2 { grid-column: span 1; }
  .method-grid { grid-template-columns: 1fr; }
  .top-actions { width: 100%; }
  .top-actions .btn { flex: 1; }
}
</style>
</head>
<body>
<div id="toast-root"></div>

<!-- 令牌输入页 -->
<div id="auth-view" style="display:none">
  <div class="auth-card">
    <div class="logo">MC</div>
    <h1>MCServerManager</h1>
    <p class="dim">请输入访问令牌进入 Web 控制台</p>
    <input id="token-input" type="password" placeholder="访问令牌 (token)" autocomplete="off">
    <button class="btn btn-primary block btn-lg" id="btn-auth">进 入</button>
    <div class="auth-err" id="auth-err"></div>
  </div>
</div>

<!-- 主界面 -->
<div id="app" style="display:none">
  <aside id="sidebar">
    <div class="brand">
      <div class="logo">MC</div>
      <div>
        <div class="brand-name">MCServerManager</div>
        <div class="brand-sub">Web 控制台</div>
      </div>
    </div>
    <div class="side-actions">
      <button class="btn btn-primary block" id="btn-add">＋ 添加服务器</button>
    </div>
    <div id="server-list"></div>
    <div class="sidebar-footer">
      <button class="btn btn-ghost block" id="btn-global-settings">⚙ 　全局设置</button>
    </div>
  </aside>
  <div id="sidebar-mask"></div>

  <main id="main">
    <div id="empty-state" style="display:none">
      <div class="empty-inner">
        <div class="glyph">&gt;_</div>
        <h2>未选择服务器</h2>
        <p>从左侧列表选择一台服务器进行管理<br>或点击「添加服务器」创建新实例</p>
      </div>
    </div>

    <div id="server-view">
      <header id="topbar">
        <button id="btn-menu" class="icon-btn" title="服务器列表">☰</button>
        <div class="srv-title-wrap">
          <div class="srv-title-line">
            <h1 id="srv-name"></h1>
            <span id="srv-badge" class="badge"></span>
          </div>
          <div class="srv-meta" id="srv-meta"></div>
        </div>
        <div class="top-actions">
          <button class="btn btn-primary" data-action="start">启动</button>
          <button class="btn btn-danger" data-action="stop">停止</button>
          <button class="btn" data-action="restart">重启</button>
        </div>
      </header>

      <nav id="tabs">
        <button class="tab-btn active" data-tab="overview">概览</button>
        <button class="tab-btn" data-tab="console">控制台</button>
        <button class="tab-btn" data-tab="players">玩家</button>
        <button class="tab-btn" data-tab="properties">配置</button>
        <button class="tab-btn" data-tab="backups">备份</button>
        <button class="tab-btn" data-tab="settings">服务器设置</button>
      </nav>

      <section id="tab-overview" class="panel active">
        <div class="panel-scroll" id="overview-content"></div>
      </section>

      <section id="tab-console" class="panel">
        <div id="console-wrap">
          <div id="console-log"></div>
          <button id="back-to-bottom" style="display:none">回到底部 ↓</button>
        </div>
        <form id="cmd-form">
          <input id="cmd-input" autocomplete="off" spellcheck="false" placeholder="输入服务器指令，回车发送">
          <button type="submit" class="btn btn-primary" id="cmd-send">发送</button>
        </form>
      </section>

      <section id="tab-players" class="panel">
        <div class="panel-scroll">
          <div class="panel-head">
            <h3 id="players-title">在线玩家</h3>
            <button class="btn" id="players-refresh">刷新列表</button>
          </div>
          <div id="players-content"></div>
        </div>
      </section>

      <section id="tab-properties" class="panel">
        <div class="panel-scroll">
          <div class="inline-msg warn" id="props-banner"></div>
          <div class="inline-msg" id="props-msg"></div>
          <div class="panel-head">
            <h3>server.properties</h3>
            <button class="btn btn-primary" id="props-save">保存修改</button>
          </div>
          <div class="table-wrap">
            <table class="kv-table"><tbody id="props-tbody"></tbody></table>
          </div>
          <div class="props-add">
            <input id="prop-new-key" placeholder="新键名" spellcheck="false" autocomplete="off">
            <input id="prop-new-value" placeholder="值" spellcheck="false" autocomplete="off">
            <button class="btn" id="prop-add-btn">添加键值对</button>
          </div>
        </div>
      </section>

      <section id="tab-backups" class="panel">
        <div class="panel-scroll" id="backups-content"></div>
      </section>

      <section id="tab-settings" class="panel">
        <div class="panel-scroll">
          <div class="card">
            <h3 class="card-title">服务器设置</h3>
            <div class="inline-msg" id="sv-msg"></div>
            <div class="form-grid">
              <label class="fld"><span>服务器名称</span><input id="sv-name" spellcheck="false"></label>
              <label class="fld"><span>Java 路径</span><input id="sv-java" spellcheck="false"></label>
              <label class="fld"><span>最小内存 (MB)</span><input id="sv-min" type="number" min="0"></label>
              <label class="fld"><span>最大内存 (MB)</span><input id="sv-max" type="number" min="0"></label>
              <label class="fld span2"><span>JVM 附加参数</span><input id="sv-jvm" spellcheck="false"></label>
              <label class="fld span2"><span>备份目录</span><input id="sv-backup" spellcheck="false"></label>
              <label class="fld span2"><span>世界目录</span><input id="sv-world" spellcheck="false"></label>
            </div>
            <div class="form-actions">
              <button class="btn btn-primary" id="sv-save">保存设置</button>
            </div>
          </div>
          <div class="card danger-zone">
            <h3 class="card-title">危险区域</h3>
            <p class="dz-desc">删除服务器会先停止运行中的进程，并<b>永久删除磁盘上的全部文件</b>。此操作不可恢复，请谨慎执行。</p>
            <button class="btn btn-danger" id="sv-delete">删除服务器</button>
          </div>
        </div>
      </section>
    </div>
  </main>
</div>

<!-- 添加服务器向导弹窗 -->
<div class="modal-mask" id="modal-add" style="display:none">
  <div class="modal modal-lg">
    <div class="modal-head">
      <h3>添加服务器</h3>
      <button class="modal-x" id="add-x" title="关闭">×</button>
    </div>
    <div class="modal-body" id="add-body"></div>
    <div class="modal-foot" id="add-foot"></div>
  </div>
</div>

<!-- 全局设置弹窗 -->
<div class="modal-mask" id="modal-settings" style="display:none">
  <div class="modal">
    <div class="modal-head">
      <h3>全局设置</h3>
      <button class="modal-x" data-close="modal-settings" title="关闭">×</button>
    </div>
    <div class="modal-body">
      <div class="inline-msg" id="set-msg"></div>
      <label class="fld">
        <span>服务器存放目录</span>
        <input id="set-folder" spellcheck="false">
        <small id="set-folder-hint"></small>
      </label>
      <label class="fld">
        <span>全局 Java 路径</span>
        <input id="set-java" spellcheck="false">
        <small id="set-java-hint"></small>
      </label>
      <label class="fld">
        <span>Web 端口</span>
        <input id="set-port" type="number" min="1" max="65535">
        <small>修改端口需重启 web 服务后生效</small>
      </label>
    </div>
    <div class="modal-foot">
      <button class="btn" data-close="modal-settings">取消</button>
      <button class="btn btn-primary" id="set-save">保存</button>
    </div>
  </div>
</div>

<!-- EULA 确认弹窗 -->
<div class="modal-mask" id="modal-eula" style="display:none">
  <div class="modal">
    <div class="modal-head">
      <h3>需要同意 Minecraft EULA</h3>
      <button class="modal-x" data-close="modal-eula" title="关闭">×</button>
    </div>
    <div class="modal-body">
      <p>启动服务器「<b id="eula-srv"></b>」前，需要先同意 Mojang 的《Minecraft 最终用户许可协议》（EULA）。</p>
      <p class="eula-link">协议全文：<span class="mono">https://aka.ms/MinecraftEULA</span></p>
      <p class="dim">点击「同意并启动」会将服务器目录下 eula.txt 中的 eula 置为 true，并立即启动服务器。</p>
    </div>
    <div class="modal-foot">
      <button class="btn" data-close="modal-eula">取消</button>
      <button class="btn btn-primary" id="eula-agree">同意并启动</button>
    </div>
  </div>
</div>

<script>
'use strict';
/* ================= 工具 ================= */
const $ = id => document.getElementById(id);
const esc = s => String(s == null ? '' : s).replace(/[&<>"']/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

function toast(msg, type) {
  const root = $('toast-root');
  const el = document.createElement('div');
  el.className = 'toast ' + (type || 'info');
  el.textContent = msg;
  root.appendChild(el);
  requestAnimationFrame(() => el.classList.add('show'));
  setTimeout(() => {
    el.classList.remove('show');
    setTimeout(() => el.remove(), 260);
  }, 3600);
}

/* ================= 全局状态 ================= */
const STATE_TEXT = { STOPPED: '已停止', STARTING: '启动中', RUNNING: '运行中', STOPPING: '停止中' };
const STATE_CLASS = { STOPPED: 'stopped', STARTING: 'starting', RUNNING: 'running', STOPPING: 'stopping' };
const TAB_NAMES = ['overview', 'console', 'players', 'properties', 'backups', 'settings'];

let token = '';
let servers = [];
let current = -1;            // 当前选中的服务器下标
let currentTab = 'overview';
let pollTimer = null;
let modalDepth = 0;          // >0 时暂停列表轮询，避免打断输入
let evtSource = null;
let consoleStick = true;     // 控制台是否贴底自动滚动
let propsLoadedFor = -1;     // 配置编辑器已加载的服务器下标
let propState = null;        // {exists, entries, order, dirty:Set, newKeys:Set}
const propInputs = new Map();// key -> input 元素
let backupsFor = -1;
let lastBackups = [];
let eulaIdx = -1;
let wiz = null;              // 添加服务器向导状态

/* ================= API ================= */
async function api(path, opts) {
  opts = opts || {};
  opts.headers = Object.assign({
    'Authorization': 'Bearer ' + token,
    'Content-Type': 'application/json'
  }, opts.headers || {});
  const res = await fetch(path, opts);
  if (res.status === 401) {
    localStorage.removeItem('mcsm_token');
    token = '';
    showAuth('令牌无效，请重新输入');
    throw new Error('unauthorized');
  }
  let data = null;
  try { data = await res.json(); } catch (e) { /* 非 JSON 响应 */ }
  if (!res.ok) {
    const err = new Error((data && data.error) ? data.error : ('HTTP ' + res.status));
    err.status = res.status;
    err.data = data;
    throw err;
  }
  return data;
}

/* ================= 认证 ================= */
function showAuth(err) {
  stopPolling();
  closeStream();
  $('app').style.display = 'none';
  $('auth-view').style.display = 'flex';
  $('auth-err').textContent = err || '';
  setTimeout(() => $('token-input').focus(), 60);
}

function saveToken() {
  const v = $('token-input').value.trim();
  if (!v) { $('auth-err').textContent = '请输入访问令牌'; return; }
  token = v;
  localStorage.setItem('mcsm_token', v);
  $('token-input').value = '';
  $('auth-err').textContent = '';
  boot();
}

/* ================= 启动与轮询 ================= */
async function boot() {
  if (!token) { showAuth(''); return; }
  try {
    servers = await api('/api/servers');
  } catch (e) {
    if (e.message !== 'unauthorized') showAuth('无法连接服务器，请确认 web 服务正在运行');
    return;
  }
  $('auth-view').style.display = 'none';
  $('app').style.display = 'flex';
  renderSidebar();
  if (servers.length > 0 && (current < 0 || current >= servers.length)) {
    current = -1;
    await selectServer(0);
  } else {
    renderMain();
  }
  startPolling();
}

function startPolling() {
  stopPolling();
  pollTimer = setInterval(refresh, 3000);
}
function stopPolling() {
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
}

async function refresh(force) {
  if (!force && modalDepth > 0) return; // 弹窗打开时暂停轮询
  try {
    servers = await api('/api/servers');
  } catch (e) { return; }
  if (current >= servers.length) current = servers.length - 1;
  renderSidebar();
  renderMain();
}

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

async function selectServer(i) {
  if (i < 0 || i >= servers.length) return;
  const changed = i !== current;
  current = i;
  closeSidebarMobile();
  if (changed) {
    closeStream();
    propsLoadedFor = -1;
    propState = null;
    backupsFor = -1;
    lastBackups = [];
    fillServerForm();
  }
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
  if (current < 0) return;
  if (tab === 'console') await loadConsole();
  else if (tab === 'properties') await loadProperties(false);
  else if (tab === 'backups') await loadBackups();
  else if (tab === 'players') renderPlayers();
  else if (tab === 'overview') renderOverview();
  else if (tab === 'settings') fillServerForm();
}

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

/* ================= 控制台 ================= */
function logClass(line) {
  if (/\b(ERROR|FATAL|SEVERE)\b/.test(line) || /Exception|Error:/.test(line)) return ' log-err';
  if (/\bWARN(ING)?\b/.test(line)) return ' log-warn';
  return '';
}

async function loadConsole() {
  if (current < 0) return;
  const idx = current;
  closeStream();
  const box = $('console-log');
  box.innerHTML = '';
  consoleStick = true;
  updateBackToBottom();
  try {
    const data = await api('/api/servers/' + idx + '/console?tail=500');
    if (current !== idx || currentTab !== 'console') return;
    (data.lines || []).forEach(line => appendConsoleLine(line));
    scrollConsoleToBottom();
  } catch (e) { /* 历史读取失败时仍可接收实时日志 */ }
  if (current === idx && currentTab === 'console') openStream(idx);
}

function openStream(idx) {
  closeStream();
  evtSource = new EventSource('/api/servers/' + idx + '/console/stream?token=' + encodeURIComponent(token));
  evtSource.onmessage = ev => {
    try {
      const line = JSON.parse(ev.data).line;
      appendConsoleLine(line);
    } catch (e) { /* 忽略解析失败 */ }
  };
}

function closeStream() {
  if (evtSource) { evtSource.close(); evtSource = null; }
}

function appendConsoleLine(line) {
  const box = $('console-log');
  if (!box) return;
  const div = document.createElement('div');
  div.className = 'log-line' + logClass(line);
  div.textContent = line;
  box.appendChild(div);
  while (box.childElementCount > 3000) box.removeChild(box.firstChild);
  if (consoleStick) scrollConsoleToBottom();
}

function scrollConsoleToBottom() {
  const box = $('console-log');
  box.scrollTop = box.scrollHeight;
}

function updateBackToBottom() {
  $('back-to-bottom').style.display = consoleStick ? 'none' : 'inline-flex';
}

async function sendCommand(cmd) {
  if (current < 0 || !cmd) return;
  try {
    await api('/api/servers/' + current + '/command', { method: 'POST', body: JSON.stringify({ command: cmd }) });
  } catch (e) {
    if (e.data && e.data.error === 'not_running') toast('服务器未运行，指令未发送', 'warn');
    else toast('指令发送失败: ' + e.message, 'error');
  }
}

/* ================= 玩家 ================= */
function renderPlayers() {
  const s = servers[current];
  const box = $('players-content');
  if (!s) { box.innerHTML = ''; return; }
  $('players-title').textContent = '在线玩家' + (s.state === 'RUNNING' ? '（' + (s.players || []).length + '）' : '');
  box.innerHTML = '';
  if (s.state !== 'RUNNING') {
    box.innerHTML = '<div class="side-empty">服务器未运行，启动后可查看在线玩家</div>';
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

/* ================= server.properties 配置 ================= */
function setInlineMsg(id, type, text) {
  const el = $(id);
  el.className = 'inline-msg ' + type;
  el.textContent = text;
}

async function loadProperties(force) {
  if (current < 0) return;
  if (!force && propsLoadedFor === current) return;
  const idx = current;
  $('props-msg').className = 'inline-msg';
  let d;
  try {
    d = await api('/api/servers/' + idx + '/properties');
  } catch (e) {
    if (current === idx) setInlineMsg('props-msg', 'err', '配置加载失败: ' + e.message);
    return;
  }
  if (current !== idx) return;
  propState = {
    exists: !!d.exists,
    entries: d.entries || {},
    order: d.order || Object.keys(d.entries || {}),
    dirty: new Set(),
    newKeys: new Set()
  };
  propsLoadedFor = idx;
  renderProperties();
}

function renderProperties() {
  const tbody = $('props-tbody');
  tbody.innerHTML = '';
  propInputs.clear();
  if (!propState) return;
  $('props-banner').className = propState.exists ? 'inline-msg' : 'inline-msg warn';
  if (!propState.exists) $('props-banner').textContent = 'server.properties 文件不存在，将在保存时创建';
  propState.order.forEach(k => addPropRow(k, propState.entries[k] != null ? propState.entries[k] : '', false));
  propState.newKeys.forEach(k => addPropRow(k, propInputs.has(k) ? propInputs.get(k).value : '', true));
}

function addPropRow(key, value, isNew) {
  if (propInputs.has(key)) return;
  const tr = document.createElement('tr');
  if (isNew) { tr.classList.add('new-row'); propState.dirty.add(key); }
  const tdK = document.createElement('td');
  tdK.className = 'prop-key';
  tdK.textContent = key;
  const tdV = document.createElement('td');
  const input = document.createElement('input');
  input.value = value;
  input.spellcheck = false;
  input.autocomplete = 'off';
  input.addEventListener('input', () => {
    const dirty = propState.newKeys.has(key) || input.value !== (propState.entries[key] != null ? propState.entries[key] : '');
    if (dirty) propState.dirty.add(key); else propState.dirty.delete(key);
    tr.classList.toggle('dirty', dirty && !propState.newKeys.has(key));
  });
  tdV.appendChild(input);
  tr.appendChild(tdK);
  tr.appendChild(tdV);
  $('props-tbody').appendChild(tr);
  propInputs.set(key, input);
}

function addNewProp() {
  if (!propState) return;
  const k = $('prop-new-key').value.trim();
  const v = $('prop-new-value').value;
  if (!k) { setInlineMsg('props-msg', 'err', '键名不能为空'); return; }
  if (/\s/.test(k)) { setInlineMsg('props-msg', 'err', '键名不能包含空白字符'); return; }
  if (propInputs.has(k) || Object.prototype.hasOwnProperty.call(propState.entries, k)) {
    setInlineMsg('props-msg', 'err', '键「' + k + '」已存在');
    return;
  }
  $('props-msg').className = 'inline-msg';
  propState.newKeys.add(k);
  propState.dirty.add(k);
  const tr = document.createElement('tr');
  tr.classList.add('new-row');
  const tdK = document.createElement('td');
  tdK.className = 'prop-key';
  tdK.textContent = k;
  const tdV = document.createElement('td');
  const input = document.createElement('input');
  input.value = v;
  input.spellcheck = false;
  input.autocomplete = 'off';
  tdV.appendChild(input);
  tr.appendChild(tdK);
  tr.appendChild(tdV);
  $('props-tbody').appendChild(tr);
  propInputs.set(k, input);
  $('prop-new-key').value = '';
  $('prop-new-value').value = '';
  setInlineMsg('props-msg', 'ok', '已添加新键「' + k + '」，点击「保存修改」后生效');
}

async function saveProperties() {
  if (current < 0 || !propState) return;
  if (propState.dirty.size === 0) {
    setInlineMsg('props-msg', 'warn', '没有需要保存的修改');
    return;
  }
  const entries = {};
  propState.dirty.forEach(k => { entries[k] = propInputs.get(k).value; });
  const n = propState.dirty.size;
  const btn = $('props-save');
  btn.disabled = true;
  try {
    await api('/api/servers/' + current + '/properties', { method: 'PUT', body: JSON.stringify({ entries: entries }) });
    setInlineMsg('props-msg', 'ok', '保存成功，共写入 ' + n + ' 项');
    toast('配置已保存', 'success');
    propsLoadedFor = -1;
    await loadProperties(true);
  } catch (e) {
    setInlineMsg('props-msg', 'err', '保存失败: ' + e.message);
  } finally {
    btn.disabled = false;
  }
}

/* ================= 备份 ================= */
async function loadBackups() {
  if (current < 0) return;
  const idx = current;
  try {
    const d = await api('/api/servers/' + idx + '/backups');
    if (current !== idx) return;
    lastBackups = d.backups || [];
    backupsFor = idx;
  } catch (e) {
    if (current === idx) toast('备份列表加载失败: ' + e.message, 'error');
  }
  renderBackupsPanel();
}

function renderBackupsPanel() {
  const s = servers[current];
  const box = $('backups-content');
  if (!s) { box.innerHTML = ''; return; }
  const stopped = s.state === 'STOPPED';
  let html = '<div class="card"><h3 class="card-title">备份信息</h3><div class="info-grid">' +
    '<div class="info-item"><div class="k">备份目录</div><div class="v mono">' + esc(s.backup_dir || '-') + '</div></div>' +
    '<div class="info-item"><div class="k">世界目录</div><div class="v mono">' + esc(s.world_dir || '-') + '</div></div>' +
    '</div></div>';
  if (!stopped) {
    html += '<div class="inline-msg warn">服务器正在运行，需先停止服务器才能创建或恢复备份</div>';
  }
  html += '<div class="panel-head"><h3>备份列表（' + lastBackups.length + '）</h3>' +
    '<button class="btn btn-primary" id="backup-create"' + (stopped ? '' : ' disabled') + '>创建备份</button></div>' +
    '<div id="backup-list"></div>';
  box.innerHTML = html;
  const listEl = $('backup-list');
  if (backupsFor !== current) {
    listEl.innerHTML = '<div class="side-empty">加载中…</div>';
  } else if (lastBackups.length === 0) {
    listEl.innerHTML = '<div class="side-empty">暂无备份</div>';
  } else {
    lastBackups.forEach(name => {
      const row = document.createElement('div');
      row.className = 'backup-row';
      const nm = document.createElement('span');
      nm.className = 'bname';
      nm.textContent = name;
      const restore = document.createElement('button');
      restore.className = 'btn';
      restore.textContent = '恢复';
      restore.disabled = !stopped;
      restore.addEventListener('click', () => restoreBackup(name));
      const del = document.createElement('button');
      del.className = 'btn btn-danger';
      del.textContent = '删除';
      del.addEventListener('click', () => deleteBackup(name));
      row.appendChild(nm);
      row.appendChild(restore);
      row.appendChild(del);
      listEl.appendChild(row);
    });
  }
  $('backup-create').addEventListener('click', createBackup);
}

async function createBackup() {
  if (current < 0) return;
  const btn = $('backup-create');
  btn.disabled = true;
  btn.textContent = '创建中…';
  try {
    const r = await api('/api/servers/' + current + '/backups', { method: 'POST', body: '{}' });
    toast('备份已创建: ' + (r.name || ''), 'success');
  } catch (e) {
    toast('创建备份失败: ' + e.message, 'error');
  }
  await loadBackups();
}

async function restoreBackup(name) {
  if (current < 0) return;
  if (!confirm('确定恢复备份「' + name + '」吗？\n当前世界数据将被该备份覆盖。')) return;
  try {
    await api('/api/servers/' + current + '/backups/' + encodeURIComponent(name) + '/restore', { method: 'POST', body: '{}' });
    toast('备份已恢复', 'success');
  } catch (e) {
    toast('恢复失败: ' + e.message, 'error');
  }
  await loadBackups();
}

async function deleteBackup(name) {
  if (current < 0) return;
  if (!confirm('确定删除备份「' + name + '」吗？此操作不可恢复。')) return;
  try {
    await api('/api/servers/' + current + '/backups/' + encodeURIComponent(name), { method: 'DELETE' });
    toast('备份已删除', 'success');
  } catch (e) {
    toast('删除失败: ' + e.message, 'error');
  }
  await loadBackups();
}

/* ================= 服务器设置 / 删除 ================= */
function fillServerForm() {
  const s = servers[current];
  if (!s) return;
  $('sv-name').value = s.name || '';
  $('sv-java').value = s.java_path || '';
  $('sv-min').value = s.min_memory_mb != null ? s.min_memory_mb : '';
  $('sv-max').value = s.max_memory_mb != null ? s.max_memory_mb : '';
  $('sv-jvm').value = s.additional_jvm_args || '';
  $('sv-backup').value = s.backup_dir || '';
  $('sv-world').value = s.world_dir || '';
  $('sv-msg').className = 'inline-msg';
}

async function saveServerConfig() {
  const s = servers[current];
  if (!s) return;
  const name = $('sv-name').value.trim();
  const minMem = parseInt($('sv-min').value, 10);
  const maxMem = parseInt($('sv-max').value, 10);
  if (!name) { setInlineMsg('sv-msg', 'err', '服务器名称不能为空'); return; }
  if (isNaN(minMem) || isNaN(maxMem) || minMem < 0 || maxMem < 0) {
    setInlineMsg('sv-msg', 'err', '内存配置无效，请输入不小于 0 的数字');
    return;
  }
  if (maxMem < minMem) { setInlineMsg('sv-msg', 'err', '最大内存不能小于最小内存'); return; }
  const body = {
    name: name,
    java_path: $('sv-java').value.trim(),
    additional_jvm_args: $('sv-jvm').value,
    backup_dir: $('sv-backup').value.trim(),
    world_dir: $('sv-world').value.trim(),
    min_memory_mb: minMem,
    max_memory_mb: maxMem
  };
  const btn = $('sv-save');
  btn.disabled = true;
  try {
    await api('/api/servers/' + current + '/config', { method: 'PUT', body: JSON.stringify(body) });
    setInlineMsg('sv-msg', 'ok', '设置已保存');
    toast('服务器设置已保存', 'success');
    await refresh();
  } catch (e) {
    setInlineMsg('sv-msg', 'err', '保存失败: ' + e.message);
  } finally {
    btn.disabled = false;
  }
}

async function deleteServer() {
  const s = servers[current];
  if (!s) return;
  if (!confirm('确定删除服务器「' + s.name + '」吗？')) return;
  if (!confirm('最后确认：删除将停止服务器，并永久删除磁盘上的全部文件：\n' + (s.working_dir || '') + '\n\n此操作不可恢复！')) return;
  try {
    await api('/api/servers/' + current, { method: 'DELETE' });
    toast('服务器已删除', 'success');
  } catch (e) {
    toast('删除失败: ' + e.message, 'error');
    return;
  }
  current = -1;
  closeStream();
  await refresh();
}

/* ================= 启停操作 / EULA ================= */
async function doAction(action) {
  if (current < 0) return;
  const idx = current;
  try {
    await api('/api/servers/' + idx + '/' + action, { method: 'POST', body: '{}' });
    toast(action === 'start' ? '启动指令已发送' : action === 'stop' ? '停止指令已发送' : '重启指令已发送', 'success');
  } catch (e) {
    if (e.status === 409 && e.data && e.data.error === 'eula_required') {
      eulaIdx = idx;
      $('eula-srv').textContent = (servers[idx] && servers[idx].name) || '';
      openModal('modal-eula');
      return;
    }
    toast('操作失败: ' + e.message, 'error');
    return;
  }
  setTimeout(refresh, 400);
}

async function agreeEula() {
  if (eulaIdx < 0) return;
  try {
    await api('/api/servers/' + eulaIdx + '/eula', { method: 'POST', body: '{}' });
    toast('已同意 EULA，服务器启动中', 'success');
    closeModal('modal-eula');
    setTimeout(refresh, 400);
  } catch (e) {
    toast('EULA 操作失败: ' + e.message, 'error');
  }
}

/* ================= 弹窗通用 ================= */
function openModal(id) {
  const m = $(id);
  if (m.style.display !== 'flex') { m.style.display = 'flex'; modalDepth++; }
}
function closeModal(id) {
  const m = $(id);
  if (m.style.display !== 'none') { m.style.display = 'none'; modalDepth = Math.max(0, modalDepth - 1); }
}

/* ================= 添加服务器向导 ================= */
const WIZ_METHODS = [
  { id: 'auto', title: '自动安装', desc: '从官方源查询版本并下载安装服务端核心（Vanilla / Paper / Forge / NeoForge / Fabric）' },
  { id: 'import', title: '导入目录', desc: '导入已有的服务器目录，自动识别核心类型与版本' },
  { id: 'installer', title: '安装器 JAR', desc: '运行本地安装器 JAR（如 Forge / NeoForge 安装器）' },
  { id: 'jar', title: '核心 JAR', desc: '使用本地已有的服务端 JAR 文件创建实例' },
];
const PATH_LABELS = { import: '服务器目录路径', installer: '安装器 JAR 路径', jar: '核心 JAR 路径' };

function resetWiz() {
  if (wiz && wiz.taskTimer) clearInterval(wiz.taskTimer);
  wiz = {
    step: 1, method: null,
    coretypes: (wiz && wiz.coretypes) || null,
    core: '', version: '', versions: [], versionsLoading: false,
    path: '', name: '', java: 'java', min: 1024, max: 4096, jvm: '',
    err: '', phase: 'edit', taskId: null, taskStatus: '', taskError: '',
    lines: [], logShown: 0, taskTimer: null
  };
}

function openAddModal() {
  resetWiz();
  renderWiz();
  openModal('modal-add');
}

// 受保护关闭：安装进行中禁止关闭
function closeAddModal() {
  if (wiz && wiz.phase === 'running') { toast('安装进行中，暂时无法关闭', 'warn'); return; }
  closeWizForce();
}

function closeWizForce() {
  if (wiz && wiz.taskTimer) { clearInterval(wiz.taskTimer); wiz.taskTimer = null; }
  closeModal('modal-add');
}

function renderWiz() {
  const body = $('add-body');
  const foot = $('add-foot');
  let dots = '<div class="wiz-steps">';
  ['选择方式', '填写信息', '安装'].forEach((t, i) => {
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

  // 第 3 步：安装进度
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
    foot.innerHTML = '<span class="foot-note">安装进行中，请勿关闭本窗口…</span>';
  } else if (wiz.phase === 'done') {
    foot.innerHTML = '<span class="foot-note">服务器「' + esc(wiz.name) + '」已就绪</span>' +
                     '<button class="btn btn-primary" id="wz-finish">完成</button>';
    $('wz-finish').addEventListener('click', closeWizForce);
  } else {
    foot.innerHTML = '<span class="foot-note">安装失败，可关闭后修正信息重试</span>' +
                     '<button class="btn" id="wz-close2">关闭</button>';
    $('wz-close2').addEventListener('click', closeWizForce);
  }
}

function renderWizStep2(body, foot, dots) {
  let h = dots;
  if (wiz.err) h += '<div class="inline-msg err">' + esc(wiz.err) + '</div>';
  if (wiz.method === 'auto') {
    h += '<label class="fld"><span>核心类型</span><select id="wz-core">' + coreOptionsHtml() + '</select></label>';
    h += '<label class="fld"><span>游戏版本</span><select id="wz-version"' + (wiz.versionsLoading ? ' disabled' : '') + '>' +
         versionOptionsHtml() + '</select>' +
         (wiz.versionsLoading ? '<small>正在查询版本列表，可能需要几秒钟…</small>' : '') + '</label>';
  } else {
    h += '<label class="fld"><span>' + esc(PATH_LABELS[wiz.method]) + '</span>' +
         '<input id="wz-path" spellcheck="false" value="' + esc(wiz.path) + '" placeholder="' +
         (wiz.method === 'import' ? '/path/to/server 目录' : '/path/to/file.jar') + '"></label>';
  }
  h += '<label class="fld"><span>服务器名称</span><input id="wz-name" spellcheck="false" value="' + esc(wiz.name) + '" placeholder="例如 survival-1.20.1"></label>';
  h += '<div class="fld-row">' +
       '<label class="fld"><span>最小内存 (MB)</span><input id="wz-min" type="number" min="0" value="' + esc(String(wiz.min)) + '"></label>' +
       '<label class="fld"><span>最大内存 (MB)</span><input id="wz-max" type="number" min="0" value="' + esc(String(wiz.max)) + '"></label>' +
       '</div>';
  h += '<label class="fld"><span>Java 路径</span><input id="wz-java" spellcheck="false" value="' + esc(wiz.java) + '" placeholder="java"></label>';
  h += '<label class="fld"><span>JVM 附加参数</span><input id="wz-jvm" spellcheck="false" value="' + esc(wiz.jvm) + '" placeholder="可留空，例如 -XX:+UseG1GC"></label>';
  body.innerHTML = h;

  const bind = (id, fn) => { const el = $(id); if (el) el.addEventListener('input', fn); };
  bind('wz-path', e => { wiz.path = e.target.value; });
  bind('wz-name', e => { wiz.name = e.target.value; });
  bind('wz-java', e => { wiz.java = e.target.value; });
  bind('wz-min', e => { wiz.min = e.target.value; });
  bind('wz-max', e => { wiz.max = e.target.value; });
  bind('wz-jvm', e => { wiz.jvm = e.target.value; });
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

  foot.innerHTML = '<button class="btn" id="wz-back">上一步</button>' +
                   '<button class="btn btn-primary" id="wz-install">开始安装</button>';
  $('wz-back').addEventListener('click', () => { wiz.step = 1; wiz.err = ''; renderWiz(); });
  $('wz-install').addEventListener('click', startInstall);
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
  wiz.versionsLoading = true;
  wiz.versions = [];
  renderWiz();
  try {
    const d = await api('/api/versions?core=' + encodeURIComponent(core));
    wiz.versions = d.versions || [];
  } catch (e) {
    wiz.versions = [];
    toast('版本列表加载失败: ' + e.message, 'error');
  }
  wiz.versionsLoading = false;
  if (wiz && wiz.step === 2 && wiz.method === 'auto' && wiz.core === core) renderWiz();
}

async function startInstall() {
  const name = wiz.name.trim();
  const minMem = parseInt(wiz.min, 10);
  const maxMem = parseInt(wiz.max, 10);
  if (!name) { wiz.err = '请填写服务器名称'; renderWiz(); return; }
  if (/[\/\\]/.test(name) || name.indexOf('..') >= 0) { wiz.err = '服务器名称不能包含 /、\\ 或 ..'; renderWiz(); return; }
  if (isNaN(minMem) || isNaN(maxMem) || minMem < 0 || maxMem < 0) { wiz.err = '内存配置无效，请输入不小于 0 的数字'; renderWiz(); return; }
  if (maxMem < minMem) { wiz.err = '最大内存不能小于最小内存'; renderWiz(); return; }
  if (wiz.method === 'auto') {
    if (!wiz.core) { wiz.err = '请选择核心类型'; renderWiz(); return; }
    if (!wiz.version) { wiz.err = '请选择游戏版本'; renderWiz(); return; }
  } else if (!wiz.path.trim()) {
    wiz.err = '请填写' + PATH_LABELS[wiz.method];
    renderWiz();
    return;
  }

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
  } else {
    body.path = wiz.path.trim();
  }

  wiz.step = 3;
  wiz.phase = 'running';
  wiz.lines = [];
  wiz.logShown = 0;
  wiz.taskError = '';
  renderWiz();
  try {
    const r = await api('/api/servers', { method: 'POST', body: JSON.stringify(body) });
    wiz.taskId = r.task_id;
  } catch (e) {
    wiz.step = 2;
    wiz.phase = 'edit';
    wiz.err = '创建安装任务失败: ' + e.message;
    renderWiz();
    return;
  }
  appendWizLogLine('安装任务已创建（#' + wiz.taskId + '），开始执行…');
  wiz.taskTimer = setInterval(pollWizTask, 1000);
}

async function pollWizTask() {
  if (!wiz || wiz.taskId == null) return;
  let t;
  try {
    t = await api('/api/tasks/' + wiz.taskId);
  } catch (e) { return; } // 单次失败忽略，等待下个周期重试
  const log = t.log || [];
  for (let i = wiz.logShown; i < log.length; i++) appendWizLogLine(log[i]);
  if (log.length > wiz.logShown) wiz.logShown = log.length;
  if (t.status === 'running') return;
  clearInterval(wiz.taskTimer);
  wiz.taskTimer = null;
  wiz.taskStatus = t.status;
  wiz.taskError = t.error || '';
  wiz.phase = t.status === 'done' ? 'done' : 'error';
  renderWiz();
  if (wiz.phase === 'done') {
    toast('服务器安装完成', 'success');
    refresh(true);
  } else {
    toast('安装失败: ' + (wiz.taskError || '未知错误'), 'error');
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
    el.innerHTML = '<span class="spinner"></span><span>安装进行中…</span>';
  } else if (wiz.phase === 'done') {
    el.className = 'wiz-status ok';
    el.textContent = '✔ 安装完成，服务器已添加到列表';
  } else if (wiz.phase === 'error') {
    el.className = 'wiz-status err';
    el.textContent = '✘ 安装失败: ' + (wiz.taskError || '未知错误');
  }
}

/* ================= 全局设置弹窗 ================= */
async function openSettingsModal() {
  openModal('modal-settings');
  $('set-msg').className = 'inline-msg';
  $('set-folder-hint').textContent = '';
  $('set-java-hint').textContent = '';
  try {
    const d = await api('/api/settings');
    $('set-folder').value = d.servers_parent_folder || '';
    $('set-folder').placeholder = d.effective_parent_folder || '默认目录';
    $('set-folder-hint').textContent = '留空使用默认目录。当前生效：' + (d.effective_parent_folder || '-');
    $('set-java').value = d.global_java_path || '';
    $('set-java').placeholder = d.effective_java_path || 'java';
    $('set-java-hint').textContent = '留空自动探测。当前生效：' + (d.effective_java_path || '-');
    $('set-port').value = d.web_port != null ? d.web_port : '';
  } catch (e) {
    setInlineMsg('set-msg', 'err', '设置加载失败: ' + e.message);
  }
}

async function saveSettings() {
  const port = parseInt($('set-port').value, 10);
  if (isNaN(port) || port <= 0 || port > 65535) {
    setInlineMsg('set-msg', 'err', 'Web 端口无效（1-65535）');
    return;
  }
  const body = {
    servers_parent_folder: $('set-folder').value.trim(),
    global_java_path: $('set-java').value.trim(),
    web_port: port
  };
  const btn = $('set-save');
  btn.disabled = true;
  try {
    const r = await api('/api/settings', { method: 'PUT', body: JSON.stringify(body) });
    setInlineMsg('set-msg', 'ok', '已保存' + (r.note ? '：' + r.note : ''));
    toast('全局设置已保存', 'success');
  } catch (e) {
    setInlineMsg('set-msg', 'err', '保存失败: ' + e.message);
  } finally {
    btn.disabled = false;
  }
}

/* ================= 事件绑定与初始化 ================= */
function bindStaticEvents() {
  $('btn-auth').addEventListener('click', saveToken);
  $('token-input').addEventListener('keydown', e => { if (e.key === 'Enter') saveToken(); });
  $('btn-add').addEventListener('click', openAddModal);
  $('btn-global-settings').addEventListener('click', openSettingsModal);
  $('btn-menu').addEventListener('click', () => $('app').classList.toggle('sidebar-open'));
  $('sidebar-mask').addEventListener('click', closeSidebarMobile);
  document.querySelectorAll('.tab-btn').forEach(b =>
    b.addEventListener('click', () => switchTab(b.getAttribute('data-tab'))));
  // 启动 / 停止 / 重启（顶栏与概览页共用，事件委托）
  document.addEventListener('click', e => {
    const btn = e.target.closest('[data-action]');
    if (btn && !btn.disabled) doAction(btn.getAttribute('data-action'));
  });
  $('cmd-form').addEventListener('submit', e => {
    e.preventDefault();
    const cmd = $('cmd-input').value.trim();
    if (cmd) { sendCommand(cmd); $('cmd-input').value = ''; }
  });
  $('console-log').addEventListener('scroll', () => {
    const box = $('console-log');
    consoleStick = box.scrollHeight - box.scrollTop - box.clientHeight < 48;
    updateBackToBottom();
  });
  $('back-to-bottom').addEventListener('click', () => {
    consoleStick = true;
    scrollConsoleToBottom();
    updateBackToBottom();
  });
  $('players-refresh').addEventListener('click', () => refresh(true));
  $('props-save').addEventListener('click', saveProperties);
  $('prop-add-btn').addEventListener('click', addNewProp);
  $('sv-save').addEventListener('click', saveServerConfig);
  $('sv-delete').addEventListener('click', deleteServer);
  $('set-save').addEventListener('click', saveSettings);
  $('eula-agree').addEventListener('click', agreeEula);
  $('add-x').addEventListener('click', closeAddModal);
  document.querySelectorAll('[data-close]').forEach(b =>
    b.addEventListener('click', () => closeModal(b.getAttribute('data-close'))));
  ['modal-settings', 'modal-eula'].forEach(id => {
    $(id).addEventListener('click', e => { if (e.target === $(id)) closeModal(id); });
  });
  $('modal-add').addEventListener('click', e => { if (e.target === $('modal-add')) closeAddModal(); });
  document.addEventListener('keydown', e => {
    if (e.key !== 'Escape') return;
    if ($('modal-add').style.display === 'flex') closeAddModal();
    else if ($('modal-settings').style.display === 'flex') closeModal('modal-settings');
    else if ($('modal-eula').style.display === 'flex') closeModal('modal-eula');
  });
}

(function init() {
  const q = new URLSearchParams(location.search).get('token');
  if (q) {
    token = q;
    localStorage.setItem('mcsm_token', q);
  } else {
    token = localStorage.getItem('mcsm_token') || '';
  }
  bindStaticEvents();
  boot();
})();
</script>
</body>
</html>
)HTML";

} // namespace mc_manager::web
