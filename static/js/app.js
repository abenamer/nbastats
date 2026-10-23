// Shared helpers: fetch, job polling, formatting, tables.
window.NBA = (() => {
  const pct = (x, d = 1) => (x == null || Number.isNaN(x)) ? '–' : (100 * x).toFixed(d) + '%';
  const num = (x, d = 1) => (x == null || Number.isNaN(x)) ? '–' : Number(x).toFixed(d);
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const labelOf = (season) => `${season - 1}-${String(season % 100).padStart(2, '0')}`;
  const RESULT = { missed: 'Missed playoffs', play_in: 'Lost in play-in', first_round: 'Lost 1st round', second_round: 'Lost 2nd round', conf_finals: 'Lost conf. finals', finals: 'Lost Finals', champion: 'Champion' };

  async function getJSON(url) {
    const r = await fetch(url, { headers: { Accept: 'application/json' } });
    const body = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(body.error || `${r.status} ${r.statusText}`);
    return body;
  }
  async function postJSON(url, data) {
    const r = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'application/json' }, body: JSON.stringify(data ?? {}) });
    const body = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(body.error || `${r.status} ${r.statusText}`);
    return body;
  }
  function sleep(ms) { return new Promise(res => setTimeout(res, ms)); }
  // Submit a job and poll until it reaches a terminal state.
  async function runJob(url, params, onProgress) {
    const job = await postJSON(url, params);
    let delay = 250;
    for (;;) {
      const j = await getJSON(`/api/jobs/${job.id}`);
      if (onProgress) onProgress(j);
      if (j.status === 'completed') return j;
      if (j.status === 'failed' || j.status === 'cancelled' || j.status === 'interrupted') throw new Error(`job ${j.status}: ${j.error || ''}`);
      await sleep(delay); delay = Math.min(delay * 1.5, 2000);
    }
  }
  // Tables sort on a header click: numbers as numbers (commas, +, % and · suffixes stripped), the rest as text.
  // The rows are sorted in place in the DOM, so every table built here is sortable without keeping its data.
  function table(columns, rows) {
    const th = columns.map((c, i) => `<th class="${c.left ? 'l' : ''}" data-col="${i}" title="Sort">${esc(c.label)}</th>`).join('');
    const body = rows.map(r => '<tr>' + columns.map(c => {
      const v = typeof c.value === 'function' ? c.value(r) : r[c.value];
      const cls = (c.left ? 'l ' : '') + (typeof c.cls === 'function' ? c.cls(r) : '');
      return `<td class="${cls}">${c.raw ? v : esc(v)}</td>`;
    }).join('') + '</tr>').join('');
    return `<table class="sortable"><thead><tr>${th}</tr></thead><tbody>${body}</tbody></table>`;
  }
  const sortKey = text => { const t = text.trim().replace(/^[+]/, '').replace(/,/g, '').replace(/[%·].*$/, ''); const n = t === '' ? NaN : Number(t); return Number.isNaN(n) ? null : n; };
  document.addEventListener('click', e => {
    const th = e.target.closest('table.sortable th[data-col]'); if (!th) return;
    const tbl = th.closest('table'), col = Number(th.dataset.col), tbody = tbl.tBodies[0]; if (!tbody) return;
    const dir = th.classList.contains('asc') ? 'desc' : th.classList.contains('desc') ? 'asc' : (col === 0 ? 'asc' : 'desc');
    tbl.querySelectorAll('th').forEach(h => h.classList.remove('asc', 'desc')); th.classList.add(dir);
    const rows = Array.from(tbody.rows), cell = r => (r.cells[col] ? r.cells[col].textContent : '');
    const numeric = rows.length && rows.every(r => sortKey(cell(r)) !== null || cell(r).trim() === '');
    const sign = dir === 'asc' ? 1 : -1;
    rows.sort((a, b) => {
      if (numeric) { const x = sortKey(cell(a)), y = sortKey(cell(b)); if (x === null || y === null) return x === y ? 0 : x === null ? 1 : -1; return sign * (x - y); }
      return sign * cell(a).localeCompare(cell(b), undefined, { numeric: true, sensitivity: 'base' });
    });
    rows.forEach(r => tbody.appendChild(r));
  });
  function bar(p, max = 1) { const w = Math.max(0, Math.min(100, 100 * p / max)); return `<span class="bar" style="width:${w.toFixed(0)}px"></span>${pct(p)}`; }
  function cards(el, items) { el.innerHTML = items.map(i => `<div class="card"><div class="label">${esc(i.label)}</div><div class="value">${esc(i.value)}</div>${i.sub ? `<div class="sub">${esc(i.sub)}</div>` : ''}</div>`).join(''); }
  function setStatus(el, msg, isError) { el.textContent = msg || ''; el.style.color = isError ? '#b91c1c' : ''; }
  const charts = {};
  // One chart per canvas. Chart.js registers the canvas before it finishes building, so a chart that throws
  // halfway would leave the canvas "in use"; any chart on the canvas is destroyed first, and a failure frees it.
  function chart(id, config) {
    const ctx = document.getElementById(id); if (!ctx) return null;
    const release = c => { try { c.destroy(); } catch (e) { delete Chart.instances[c.id]; } };
    const old = charts[id] || Chart.getChart(ctx); if (old) release(old); delete charts[id];
    try { charts[id] = new Chart(ctx, config); return charts[id]; }
    catch (e) { const stuck = Chart.getChart(ctx); if (stuck) release(stuck); throw e; }
  }
  // Dates on a linear axis: x is days since the epoch; ticks read as months for one season, years for longer spans.
  const DAY = 86400000;
  const dayOf = d => Date.parse(d + 'T00:00:00Z') / DAY;
  const dateOf = x => new Date(x * DAY).toISOString().slice(0, 10);
  // `hideAfter` blanks the ticks past a day (the strip that stands for the rest of a season is not to scale).
  function dateScale(days, extra = {}, hideAfter = null) {
    const min = Math.min(...days), max = Math.max(...days), span = Math.max(max - min, 1), pad = Math.max(span * 0.01, 0.5);
    const fmt = span > 800 ? x => new Date(x * DAY).getUTCFullYear()
      : x => new Date(x * DAY).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });
    // precision 0 keeps the ticks on whole days, so a short span never repeats a date.
    return { type: 'linear', min: min - pad, max: max + pad, ticks: { maxTicksLimit: 12, precision: 0, callback: v => hideAfter != null && v > hideAfter ? '' : fmt(v) }, grid: { color: '#f1f3f6' }, ...extra };
  }
  // Chart.js plugin: shade the x range from `from` to the axis end and label it (the part of a season not played yet).
  const unplayedShade = { id: 'unplayedShade', beforeDatasetsDraw(c, _args, opts) {
    if (opts.from == null) return; const x = c.scales.x, a = c.chartArea, x0 = Math.max(x.getPixelForValue(opts.from), a.left);
    if (x0 >= a.right) return; const g = c.ctx; g.save(); g.fillStyle = 'rgba(148, 163, 184, 0.12)'; g.fillRect(x0, a.top, a.right - x0, a.bottom - a.top);
    g.strokeStyle = 'rgba(148, 163, 184, 0.6)'; g.setLineDash([4, 4]); g.beginPath(); g.moveTo(x0, a.top); g.lineTo(x0, a.bottom); g.stroke();
    if (opts.label && a.right - x0 > 90) { g.fillStyle = '#64748b'; g.font = '12px system-ui, sans-serif'; g.textAlign = 'right'; g.fillText(opts.label, a.right - 6, a.top + 14); }
    g.restore(); } };
  const palette = ['#2563eb','#dc2626','#16a34a','#d97706','#7c3aed','#0891b2','#db2777','#4b5563','#65a30d','#9333ea','#ea580c','#0d9488','#1e3a8a','#991b1b','#166534','#92400e'];
  function color(i) { return palette[i % palette.length]; }
  return { pct, num, esc, labelOf, RESULT, getJSON, postJSON, runJob, table, bar, cards, setStatus, chart, color, dayOf, dateOf, dateScale, unplayedShade };
})();
