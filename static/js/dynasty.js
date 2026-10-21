(async () => {
  const { pct, num, esc, getJSON, table, cards, setStatus, chart, RESULT } = NBA;
  const status = document.getElementById('status');
  document.querySelectorAll('[data-preset]').forEach(b => b.addEventListener('click', () => { const [f, a, z] = b.dataset.preset.split(','); document.getElementById('franchise').value = f; document.getElementById('from').value = a; document.getElementById('to').value = z; run(); }));
  async function run() {
    const q = new URLSearchParams({ franchise: document.getElementById('franchise').value, from: document.getElementById('from').value, to: document.getElementById('to').value, alpha: document.getElementById('alpha').value, home_advantage: document.getElementById('adv').value, scale: document.getElementById('scale').value, threshold: document.getElementById('threshold').value });
    setStatus(status, 'Computing…');
    try {
      const d = await getJSON(`/api/dynasty?${q}`);
      const c = Object.fromEntries(d.comparisons.map(x => [x.name, x]));
      cards(document.getElementById('cards'), [
        { label: 'Actual titles', value: d.actual_titles, sub: `${d.franchise_name}, ${NBA.labelOf(d.from_season)} to ${NBA.labelOf(d.to_season)}` },
        { label: 'Expected titles (historical Elo)', value: num(c.historical_elo.expected_titles, 2), sub: 'sum of pre-playoff title probabilities' },
        { label: 'P(≥ actual | historical Elo)', value: pct(c.historical_elo.p_at_least_actual, 2), sub: 'Poisson-binomial over seasons' },
        { label: 'P(≥ actual | equal lottery)', value: pct(c.equal_lottery.p_at_least_actual, 3), sub: `expected ${num(c.equal_lottery.expected_titles, 2)} titles` },
        { label: `P(≥ actual | alpha = ${d.alpha})`, value: pct(c.persistent_alpha.p_at_least_actual, 2), sub: `expected ${num(c.persistent_alpha.expected_titles, 2)} titles` }]);
      document.getElementById('seasons').innerHTML = table([
        { label: 'Season', value: s => NBA.labelOf(s.season), left: true }, { label: 'Code', value: 'code', left: true }, { label: 'Record', value: s => `${s.wins}-${s.losses}` },
        { label: 'Reg. end Elo', value: s => num(s.elo_regular_end, 0) }, { label: 'Pre-playoff Elo', value: s => num(s.elo_pre_playoffs, 0) }, { label: 'Seed', value: s => s.seed || '–' },
        { label: 'P(title | Elo)', value: s => pct(s.p_elo, 1) }, { label: 'P(title | lottery)', value: s => pct(s.p_lottery, 2) }, { label: 'P(title | alpha)', value: s => pct(s.p_alpha, 2) },
        { label: 'Actual', value: s => RESULT[s.actual_result], left: true, cls: s => s.champion ? 'good' : '' }], d.seasons);
      chart('eloChart', { data: { labels: d.seasons.map(s => NBA.labelOf(s.season)), datasets: [
        { type: 'line', label: 'Pre-playoff Elo', data: d.seasons.map(s => s.elo_pre_playoffs), borderColor: '#2563eb', yAxisID: 'y', tension: .2 },
        { type: 'bar', label: 'P(title | Elo)', data: d.seasons.map(s => s.p_elo), backgroundColor: d.seasons.map(s => s.champion ? '#16a34a' : '#93c5fd'), yAxisID: 'y1' }] },
        options: { scales: { y: { position: 'left', title: { display: true, text: 'Elo' } }, y1: { position: 'right', min: 0, max: 1, ticks: { callback: v => pct(v, 0) }, grid: { drawOnChartArea: false } } } } });
      const n = Math.max(...d.comparisons.map(x => x.distribution.length));
      chart('distChart', { type: 'bar', data: { labels: Array.from({ length: n }, (_, i) => i), datasets: d.comparisons.map((x, i) => ({ label: x.name.replace('_', ' '), data: x.distribution, backgroundColor: NBA.color(i) })) }, options: { plugins: { title: { display: true, text: `Titles in ${d.seasons.length} seasons (actual: ${d.actual_titles})` } }, scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      const k = d.concentration;
      if (k) {
        document.getElementById('concentration').innerHTML = `<div>Seasons with Elo forecasts</div><div>${k.seasons} (${k.elo_seasons_used.map(NBA.labelOf).join(', ')})</div><div>Observed maximum titles</div><div>${k.actual_max_titles}</div><div>P(any franchise ≥ ${k.threshold}) · equal strength</div><div>${pct(k.p_any_equal, 2)}</div><div>P(any franchise ≥ ${k.threshold}) · alpha ${d.alpha} for ${esc(d.franchise_key)}</div><div>${pct(k.p_any_alpha, 2)}</div><div>P(any franchise ≥ ${k.threshold}) · historical Elo</div><div>${pct(k.p_any_elo, 2)} ± ${pct(1.96 * k.p_any_elo_se, 2)}</div>`;
        const m = Math.max(k.max_distribution_equal.length, k.max_distribution_elo.length);
        chart('maxChart', { type: 'bar', data: { labels: Array.from({ length: m }, (_, i) => i), datasets: [{ label: 'Equal', data: k.max_distribution_equal, backgroundColor: '#9ca3af' }, { label: 'Persistent alpha', data: k.max_distribution_alpha, backgroundColor: '#f59e0b' }, { label: 'Historical Elo', data: k.max_distribution_elo, backgroundColor: '#2563eb' }] }, options: { plugins: { title: { display: true, text: 'Distribution of the maximum title count across franchises' } }, scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      } else { document.getElementById('concentration').innerHTML = '<div>No complete seasons in range</div><div>–</div>'; }
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
  }
  document.getElementById('run').addEventListener('click', run);
  run();
})();
