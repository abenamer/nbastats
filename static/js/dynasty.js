(async () => {
  const { pct, num, esc, getJSON, table, cards, setStatus, chart, RESULT } = NBA;
  const status = document.getElementById('status');
  const NAMES = { equal_lottery: 'Equal lottery', persistent_alpha: 'Ticket lottery', historical_elo: 'Historical Elo' };
  const signed = (x, d = 0) => x == null ? '–' : `${x > 0 ? '+' : x < 0 ? '−' : ''}${num(Math.abs(x), d)}`;
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
        { label: `P(≥ actual | ${d.alpha} tickets)`, value: pct(c.persistent_alpha.p_at_least_actual, 2), sub: `expected ${num(c.persistent_alpha.expected_titles, 2)} titles; ${d.alpha} tickets against one for each other franchise` }]);
      // Strength in Elo: the franchise against each season's field, and the titles read backwards.
      const st = d.strength_summary;
      const bySeason = Object.fromEntries(d.strength.map(s => [s.season, s]));
      const im = d.implied && d.implied.elo && d.implied.elo.implied;
      const noReading = d.implied_note || (d.implied && !d.implied.elo ? 'no season of the window has a field of sixteen rated teams' : d.implied ? 'a title every season has no finite reading' : 'no reading');
      cards(document.getElementById('strengthCards'), st ? [
        { label: 'Elo above the league mean', value: signed(st.mean_gap_to_mean), sub: `from ${signed(st.min_gap_to_mean)} to ${signed(st.max_gap_to_mean)} over ${st.seasons_rated} rated season${st.seasons_rated === 1 ? '' : 's'}` },
        { label: 'Gap to the best other team', value: signed(st.mean_gap_to_best_other), sub: 'mean; above zero means stronger than every other team' },
        { label: 'Gap to the worst team', value: signed(st.mean_gap_to_worst), sub: 'mean over the rated seasons' },
        { label: 'Rank by pre-playoff Elo', value: num(st.mean_rank, 1), sub: `first in ${st.seasons_first} of ${st.seasons_rated}` },
        { label: 'Elo the titles imply', value: im ? signed(im.gap) : '–', sub: im ? `above the mean for ${d.actual_titles} title${d.actual_titles === 1 ? '' : 's'} in ${d.implied.alpha.seasons} against these fields; actual mean ${signed(st.mean_gap_to_mean)}` : noReading }] : []);
      document.getElementById('strengthNote').textContent = st ? (im ? `The implied gap is the one at which a team of constant strength, placed in each season's real field through a 16-team bracket, has a title chance equal to the record's (${pct(d.implied.alpha.probability, 0)} a season). Even odds for the record need ${d.implied.elo.even_odds ? signed(d.implied.elo.even_odds.gap) : '–'}; an average team's chance against these fields is ${pct(d.implied.elo.p_average, 1)}. The ratings are those of the pre-playoff moment, so a team that peaked earlier or later reads lower here.` : noReading) : 'No season of the window has a field with this franchise rated.';
      document.getElementById('seasons').innerHTML = table([
        { label: 'Season', value: s => NBA.labelOf(s.season), left: true }, { label: 'Code', value: 'code', left: true }, { label: 'Record', value: s => `${s.wins}-${s.losses}` },
        { label: 'Reg. end Elo', value: s => num(s.elo_regular_end, 0) }, { label: 'Pre-playoff Elo', value: s => num(s.elo_pre_playoffs, 0) }, { label: 'Seed', value: s => s.seed || '–' },
        { label: 'Rank', value: s => { const f = bySeason[s.season]; return f && f.rank != null ? `${f.rank} of ${f.teams}` : ''; }, title: 'Rank by pre-playoff Elo among the rated NBA teams of the season' },
        { label: 'vs mean', value: s => { const f = bySeason[s.season]; return f ? signed(f.gap_to_mean) : ''; }, cls: s => { const f = bySeason[s.season]; return f && f.gap_to_mean > 0 ? 'good' : ''; }, title: 'Pre-playoff Elo above the league mean' },
        { label: 'Best other', value: s => { const f = bySeason[s.season]; return f && f.best_other ? `${f.best_other.code} ${signed(f.gap_to_best_other)}` : ''; }, left: true, title: 'The strongest team other than this franchise, and the gap to it' },
        { label: 'Worst', value: s => { const f = bySeason[s.season]; return f && f.worst ? (f.worst.code === s.code ? 'itself' : `${f.worst.code} ${signed(f.gap_to_worst)}`) : ''; }, left: true, title: 'The weakest team of the field, and the gap to it' },
        { label: 'P(title | Elo)', value: s => pct(s.p_elo, 1) }, { label: 'P(title | lottery)', value: s => pct(s.p_lottery, 2) }, { label: 'P(title | tickets)', value: s => pct(s.p_alpha, 2), title: 'The ticket lottery: alpha tickets for this franchise against one for each other franchise that season' },
        { label: 'Actual', value: s => RESULT[s.actual_result], left: true, cls: s => s.champion ? 'good' : '' }], d.seasons);
      chart('eloChart', { data: { labels: d.seasons.map(s => NBA.labelOf(s.season)), datasets: [
        { type: 'line', label: 'Pre-playoff Elo', data: d.seasons.map(s => s.elo_pre_playoffs), borderColor: '#2563eb', yAxisID: 'y', tension: .2 },
        { type: 'bar', label: 'P(title | Elo)', data: d.seasons.map(s => s.p_elo), backgroundColor: d.seasons.map(s => s.champion ? '#16a34a' : '#93c5fd'), yAxisID: 'y1' }] },
        options: { scales: { y: { position: 'left', title: { display: true, text: 'Elo' } }, y1: { position: 'right', min: 0, max: 1, ticks: { callback: v => pct(v, 0) }, grid: { drawOnChartArea: false } } } } });
      const n = Math.max(...d.comparisons.map(x => x.distribution.length));
      chart('distChart', { type: 'bar', data: { labels: Array.from({ length: n }, (_, i) => i), datasets: d.comparisons.map((x, i) => ({ label: NAMES[x.name] || x.name.replace('_', ' '), data: x.distribution, backgroundColor: NBA.color(i) })) }, options: { plugins: { title: { display: true, text: `Titles in ${d.seasons.length} seasons (actual: ${d.actual_titles})` } }, scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      const k = d.concentration;
      if (k) {
        document.getElementById('concentration').innerHTML = `<div>Seasons with Elo forecasts</div><div>${k.seasons} (${k.elo_seasons_used.map(NBA.labelOf).join(', ')})</div><div>Observed maximum titles</div><div>${k.actual_max_titles}</div><div>P(any franchise ≥ ${k.threshold}) · equal strength</div><div>${pct(k.p_any_equal, 2)}</div><div>P(any franchise ≥ ${k.threshold}) · ${esc(d.alpha)} tickets for ${esc(d.franchise_key)}</div><div>${pct(k.p_any_alpha, 2)}</div><div>P(any franchise ≥ ${k.threshold}) · historical Elo</div><div>${pct(k.p_any_elo, 2)} ± ${pct(1.96 * k.p_any_elo_se, 2)}</div>`;
        const m = Math.max(k.max_distribution_equal.length, k.max_distribution_elo.length);
        chart('maxChart', { type: 'bar', data: { labels: Array.from({ length: m }, (_, i) => i), datasets: [{ label: 'Equal', data: k.max_distribution_equal, backgroundColor: '#9ca3af' }, { label: 'Ticket lottery', data: k.max_distribution_alpha, backgroundColor: '#f59e0b' }, { label: 'Historical Elo', data: k.max_distribution_elo, backgroundColor: '#2563eb' }] }, options: { plugins: { title: { display: true, text: 'Distribution of the maximum title count across franchises' } }, scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      } else { document.getElementById('concentration').innerHTML = '<div>No complete seasons in range</div><div>–</div>'; }
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
  }
  document.getElementById('run').addEventListener('click', run);
  run();
})();
