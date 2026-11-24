/* ================= 弹窗通用 ================= */
function openModal(id) {
  const m = $(id);
  if (m.style.display !== 'flex') { m.style.display = 'flex'; modalDepth++; }
}
function closeModal(id) {
  const m = $(id);
  if (m.style.display !== 'none') { m.style.display = 'none'; modalDepth = Math.max(0, modalDepth - 1); }
}
