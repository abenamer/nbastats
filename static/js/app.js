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
  function table(columns, rows) {
    const th = columns.map(c => `<th class="${c.left ? 'l' : ''}">${esc(c.label)}</th>`).join('');
    const body = rows.map(r => '<tr>' + columns.map(c => {
      const v = typeof c.value === 'function' ? c.value(r) : r[c.value];
      const cls = (c.left ? 'l ' : '') + (typeof c.cls === 'function' ? c.cls(r) : '');
      return `<td class="${cls}">${c.raw ? v : esc(v)}</td>`;
    }).join('') + '</tr>').join('');
    return `<table><thead><tr>${th}</tr></thead><tbody>${body}</tbody></table>`;
  }
  function bar(p, max = 1) { const w = Math.max(0, Math.min(100, 100 * p / max)); return `<span class="bar" style="width:${w.toFixed(0)}px"></span>${pct(p)}`; }
  function cards(el, items) { el.innerHTML = items.map(i => `<div class="card"><div class="label">${esc(i.label)}</div><div class="value">${esc(i.value)}</div>${i.sub ? `<div class="sub">${esc(i.sub)}</div>` : ''}</div>`).join(''); }
  function setStatus(el, msg, isError) { el.textContent = msg || ''; el.style.color = isError ? '#b91c1c' : ''; }
  const charts = {};
  function chart(id, config) { if (charts[id]) charts[id].destroy(); const ctx = document.getElementById(id); if (!ctx) return null; charts[id] = new Chart(ctx, config); return charts[id]; }
  // Dates on a linear axis: x is days since the epoch; ticks read as months for one season, years for longer spans.
  const DAY = 86400000;
  const dayOf = d => Date.parse(d + 'T00:00:00Z') / DAY;
  const dateOf = x => new Date(x * DAY).toISOString().slice(0, 10);
  function dateScale(days, extra = {}) {
    const min = Math.min(...days), max = Math.max(...days), span = max - min;
    const fmt = span > 800 ? x => new Date(x * DAY).getUTCFullYear()
      : x => new Date(x * DAY).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });
    return { type: 'linear', min: min - span * 0.01, max: max + span * 0.01, ticks: { maxTicksLimit: 12, callback: v => fmt(v) }, grid: { color: '#f1f3f6' }, ...extra };
  }
  const palette = ['#2563eb','#dc2626','#16a34a','#d97706','#7c3aed','#0891b2','#db2777','#4b5563','#65a30d','#9333ea','#ea580c','#0d9488','#1e3a8a','#991b1b','#166534','#92400e'];
  function color(i) { return palette[i % palette.length]; }
  return { pct, num, esc, labelOf, RESULT, getJSON, postJSON, runJob, table, bar, cards, setStatus, chart, color, dayOf, dateOf, dateScale };
})();
