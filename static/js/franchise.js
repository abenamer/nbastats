(async () => {
  const { num, esc, getJSON, table, cards, setStatus, chart, labelOf, RESULT } = NBA;
  const status = document.getElementById('status');
  // Each franchise draws in its own uniform colours: the darker one as the line, the lighter
  // one as the area under it. The fallback pair is the validated blue/red; the title star is
  // always gold.
  const FALLBACK = ['#2563eb', '#dc2626'];
  const STAR = '#f59e0b', STAR_EDGE = '#92400e';
  const rgb = h => [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16));
  const dist = (a, b) => { const [x, y] = [rgb(a), rgb(b)]; return Math.hypot(x[0] - y[0], x[1] - y[1], x[2] - y[2]); };
  const lum = h => { const c = rgb(h).map(v => v / 255).map(v => v <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4); return .2126 * c[0] + .7152 * c[1] + .0722 * c[2]; };
  const contrast = h => 1.05 / (lum(h) + .05);
  const alpha = (h, a) => `rgba(${rgb(h).join(',')},${a})`;
  // Darker uniform colour for the line, lighter one for the area under it.
  const styleOf = c => lum(c.primary) <= lum(c.secondary) ? { line: c.primary, area: c.secondary } : { line: c.secondary, area: c.primary };
  // Pick the second franchise's line colour so it stays distinguishable from the first.
  function pickStyle(first, second) {
    const a = first.style, b = second.style;
    if (dist(a.line, b.line) > 70) return b;
    // Same line colour (Bulls and Rockets, both black): use the other uniform colour if it reads on white.
    if (contrast(b.area) >= 3 && dist(a.line, b.area) > 70) return { line: b.area, area: '#cbd5e1' };
    return { line: dist(a.line, FALLBACK[1]) > 70 ? FALLBACK[1] : FALLBACK[0], area: '#cbd5e1' };
  }
  const BANDS = [[1700, 'All-time great'], [1600, 'Title contender'], [1500, 'Average'], [1400, 'Lottery'], [1300, 'Dreadful']];
  // A five-point star as a point style (Chart.js's built-in 'star' is an asterisk).
  const starImage = (() => { const c = document.createElement('canvas'); c.width = c.height = 24; const g = c.getContext('2d'); const cx = 12, cy = 12, R = 10, r = 4.2;
    g.beginPath(); for (let i = 0; i < 10; i++) { const a = -Math.PI / 2 + i * Math.PI / 5, d = i % 2 ? r : R; g.lineTo(cx + d * Math.cos(a), cy + d * Math.sin(a)); } g.closePath();
    g.fillStyle = STAR; g.fill(); g.lineWidth = 1.5; g.strokeStyle = STAR_EDGE; g.stroke(); return c; })();
  // Game dates become fractional years so the x axis is linear and decimation works.
  const decYear = d => { const t = Date.parse(d + 'T00:00:00Z'); const y = new Date(t).getUTCFullYear(); const a = Date.UTC(y, 0, 1), b = Date.UTC(y + 1, 0, 1); return y + (t - a) / (b - a); };
  // Band lines across the plot; their labels sit in the right-hand margin, clear of the data.
  const bands = { id: 'bands', beforeDatasetsDraw(c) {
    const { ctx, chartArea: { left, right }, scales: { y } } = c; ctx.save();
    ctx.strokeStyle = '#d7dbe2'; ctx.setLineDash([4, 4]); ctx.fillStyle = '#6b7280'; ctx.font = '11px system-ui, sans-serif'; ctx.textAlign = 'left'; ctx.textBaseline = 'middle';
    for (const [v, label] of BANDS) { if (v < y.min || v > y.max) continue; const py = y.getPixelForValue(v); ctx.beginPath(); ctx.moveTo(left, py); ctx.lineTo(right, py); ctx.stroke(); ctx.fillText(label, right + 8, py); }
    ctx.restore(); } };
  const crosshair = { id: 'crosshair', afterDraw(c) {
    const active = c.tooltip && c.tooltip.getActiveElements ? c.tooltip.getActiveElements() : []; if (!active.length) return;
    const x = active[0].element.x; const { ctx, chartArea: { top, bottom } } = c; ctx.save(); ctx.strokeStyle = 'rgba(107,114,128,.45)'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(x, top); ctx.lineTo(x, bottom); ctx.stroke(); ctx.restore(); } };

  async function load(key, idx, from, to) {
    const q = new URLSearchParams(); if (from) q.set('from', from); if (to) q.set('to', to);
    const d = await getJSON(`/api/franchises/${encodeURIComponent(key)}/elo?${q}`);
    const pts = d.points.map(p => ({ x: decYear(p.game_date), y: p.elo_post, date: p.game_date, code: p.code, season: p.season, type: p.game_type }));
    const lastOf = {}; for (const p of pts) lastOf[p.season] = p; // the last game of a season is the clinching or eliminating one
    const fk = d.franchise.key;
    const won = new Set(d.championships.filter(c => c.franchise_key === fk).map(c => c.season));
    const lost = new Set(d.championships.filter(c => c.franchise_key !== fk).map(c => c.season));
    const at = set => [...set].map(s => lastOf[s]).filter(Boolean).sort((a, b) => a.x - b.x);
    return { franchise: d.franchise, style: styleOf(d.colors), idx, pts, titles: at(won), finals: at(lost), won, lost, seasons: d.seasons };
  }
  function datasets(s, alone) {
    const name = s.franchise.name, c = s.style;
    return [
      { label: name, data: s.pts, borderColor: c.line, borderWidth: 1.3, pointRadius: 0, pointHitRadius: 6, tension: 0, order: 2, kind: 'line',
        fill: 'origin', backgroundColor: alpha(c.area, alone ? 0.28 : 0.16) },
      { label: `${name}: NBA title`, data: s.titles, type: 'scatter', pointStyle: starImage, radius: 12, hoverRadius: 14, order: 0, kind: 'title' },
      { label: `${name}: lost Finals`, data: s.finals, type: 'scatter', pointStyle: 'circle', radius: 5, hoverRadius: 7, backgroundColor: '#fff', borderColor: c.line, borderWidth: 2, order: 1, kind: 'finals' }];
  }
  async function draw() {
    const key = document.getElementById('franchise').value, cmp = document.getElementById('compare').value;
    const from = document.getElementById('from').value, to = document.getElementById('to').value;
    const btn = document.getElementById('load'); btn.disabled = true; setStatus(status, 'Loading…');
    try {
      const series = [await load(key, 0, from, to)];
      if (cmp && cmp !== key) { const second = await load(cmp, 1, from, to); second.style = pickStyle(series[0], second); series.push(second); }
      const alone = series.length === 1;
      const all = series.flatMap(s => datasets(s, alone)).filter(ds => ds.data.length);
      const summary = document.getElementById('summary'); summary.style.setProperty('--team', series[0].style.line); summary.classList.add('team-accent');
      const xs = series.flatMap(s => s.pts.map(p => p.x)); const xMin = Math.floor(Math.min(...xs)), xMax = Math.ceil(Math.max(...xs));
      chart('historyChart', { type: 'line', data: { datasets: all }, plugins: [bands, crosshair],
        options: { responsive: true, maintainAspectRatio: false, parsing: false, animation: false, normalized: true, layout: { padding: { right: 96 } }, interaction: { mode: 'nearest', axis: 'x', intersect: false },
          plugins: { legend: { display: true, labels: { usePointStyle: true, filter: item => !item.text.endsWith('lost Finals') || all[item.datasetIndex].data.length } },
            decimation: { enabled: true, algorithm: 'lttb', samples: 2500 },
            tooltip: { callbacks: {
              title: items => items.length ? items[0].raw.date : '',
              label: item => { const p = item.raw, ds = item.dataset; const s = ds.kind === 'title' ? ' · NBA champion' : ds.kind === 'finals' ? ' · lost the Finals' : '';
                const prev = ds.kind === 'line' ? ds.data[item.dataIndex - 1] : null; const delta = prev ? ` (${p.y - prev.y >= 0 ? '+' : ''}${num(p.y - prev.y, 1)})` : '';
                return `${p.code} ${num(p.y, 0)}${delta} · ${labelOf(p.season)}${p.type === 'playoff' ? ', playoffs' : ''}${s}`; } } } },
          scales: { x: { type: 'linear', min: xMin, max: xMax, ticks: { stepSize: 5, callback: v => Number.isInteger(v) ? v : '' }, grid: { color: '#f1f3f6' } },
                    y: { suggestedMin: 1250, suggestedMax: 1800, ticks: { stepSize: 100 }, grid: { color: '#f1f3f6' }, title: { display: true, text: 'Elo after the game' } } } } });
      const s = series[0];
      const peak = s.pts.reduce((a, p) => p.y > a.y ? p : a, s.pts[0]); const low = s.pts.reduce((a, p) => p.y < a.y ? p : a, s.pts[0]);
      const span = s.pts.length ? `${s.pts[0].date.slice(0, 4)}–${s.pts[s.pts.length - 1].date.slice(0, 4)}` : '–';
      cards(document.getElementById('summary'), [
        { label: 'Titles', value: String(s.titles.length), sub: s.titles.map(p => labelOf(p.season)).join(', ') || 'none in range' },
        { label: 'Finals lost', value: String(s.finals.length), sub: s.finals.map(p => labelOf(p.season)).join(', ') || 'none in range' },
        { label: 'Peak Elo', value: peak ? num(peak.y, 0) : '–', sub: peak ? `${peak.date} (${labelOf(peak.season)})` : '' },
        { label: 'Lowest Elo', value: low ? num(low.y, 0) : '–', sub: low ? `${low.date} (${labelOf(low.season)})` : '' },
        { label: 'Games', value: s.pts.length.toLocaleString(), sub: span }]);
      const deep = s.seasons.filter(r => s.won.has(r.season) || s.lost.has(r.season)).slice().sort((a, b) => b.season - a.season);
      document.getElementById('titles').innerHTML = deep.length ? table([
        { label: 'Season', value: r => labelOf(r.season), left: true }, { label: 'Code', value: 'code', left: true }, { label: 'W', value: 'wins' }, { label: 'L', value: 'losses' },
        { label: 'Pre-playoffs Elo', value: r => num(r.elo_pre_playoffs, 0) }, { label: 'Peak', value: r => num(r.elo_peak, 0) }, { label: 'Seed', value: r => r.seed || '' },
        { label: 'Result', value: r => s.won.has(r.season) ? 'NBA champion' : 'Lost the Finals', left: true, cls: r => s.won.has(r.season) ? 'good' : '' }], deep)
        : '<p class="muted">No Finals appearances in this range.</p>';
      const bySeason = s.seasons.slice().sort((a, b) => (b.elo_peak || 0) - (a.elo_peak || 0));
      const best = bySeason.slice(0, 5), worst = s.seasons.slice().sort((a, b) => (a.elo_pre_playoffs || 9999) - (b.elo_pre_playoffs || 9999)).slice(0, 5);
      document.getElementById('extremes').innerHTML = `<h3 class="small muted">Highest peaks</h3>` + table([
        { label: 'Season', value: r => labelOf(r.season), left: true }, { label: 'Code', value: 'code', left: true }, { label: 'W-L', value: r => `${r.wins}-${r.losses}`, left: true }, { label: 'Peak Elo', value: r => num(r.elo_peak, 0) }, { label: 'Result', value: r => RESULT[r.playoff_result] || r.playoff_result, left: true }], best)
        + `<h3 class="small muted" style="margin-top:1rem">Lowest pre-playoff ratings</h3>` + table([
        { label: 'Season', value: r => labelOf(r.season), left: true }, { label: 'Code', value: 'code', left: true }, { label: 'W-L', value: r => `${r.wins}-${r.losses}`, left: true }, { label: 'Pre-playoffs Elo', value: r => num(r.elo_pre_playoffs, 0) }], worst);
      setStatus(status, series.length > 1 ? `${series[0].franchise.name} vs ${series[1].franchise.name}` : '');
    } catch (e) { setStatus(status, e.message, true); } finally { btn.disabled = false; }
  }
  document.getElementById('load').addEventListener('click', draw);
  for (const id of ['franchise', 'compare']) document.getElementById(id).addEventListener('change', draw);
  draw();
})();
