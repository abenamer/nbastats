(async () => {
  const { pct, num, esc, getJSON, table, cards, setStatus, chart } = NBA;
  const status = document.getElementById('status');
  async function load() {
    const from = document.getElementById('from').value, to = document.getElementById('to').value;
    setStatus(status, 'Loading…');
    try {
      const d = await getJSON(`/api/dashboard?from=${from}&to=${to}`);
      cards(document.getElementById('cards'), [
        { label: 'Seasons with a champion', value: d.seasons_with_champion, sub: `${d.from_season}–${d.to_season}` },
        { label: 'Max titles by one franchise', value: d.max_titles, sub: d.titles[0] ? d.titles[0].name : '' },
        { label: 'P(any franchise ≥ max) if equal', value: pct(d.p_any_at_least_max_equal), sub: 'exact occupancy probability' },
        { label: 'P(any franchise ≥ 4) if equal', value: pct(d.p_any_at_least_4_equal), sub: `${d.franchise_count} franchises, ${d.seasons_with_champion} seasons` },
        { label: 'Gini (titles)', value: num(d.gini, 3), sub: `equal-strength expectation ${num(d.expected_gini_equal, 3)}` },
        { label: 'HHI (titles)', value: num(d.hhi, 3), sub: `equal-strength expectation ${num(d.expected_hhi_equal, 3)}` },
        { label: 'Top-5 share of titles', value: pct(d.top5_share), sub: `equal-strength expectation ${pct(d.expected_top5_equal)}` },
        { label: 'Games in database', value: d.game_count.toLocaleString(), sub: d.latest_season ? `latest season ${NBA.labelOf(d.latest_season)}` : '' },
      ]);
      const winners = d.titles.filter(t => t.titles > 0);
      chart('titlesChart', { type: 'bar', data: { labels: winners.map(t => t.key), datasets: [{ label: 'Championships', data: winners.map(t => t.titles), backgroundColor: '#2563eb' }] },
        options: { plugins: { legend: { display: false }, tooltip: { callbacks: { afterLabel: (c) => winners[c.dataIndex].seasons.map(NBA.labelOf).join(', ') } } }, scales: { y: { beginAtZero: true, ticks: { precision: 0 } } } } });
      const n = Math.max(d.observed_by_count.length, d.expected_by_count_equal.length);
      const labels = Array.from({ length: n }, (_, i) => `${i} title${i === 1 ? '' : 's'}`);
      chart('countChart', { type: 'bar', data: { labels, datasets: [
        { label: 'Observed franchises', data: d.observed_by_count, backgroundColor: '#2563eb' },
        { label: 'Expected if equal strength', data: d.expected_by_count_equal.map(x => +x.toFixed(2)), backgroundColor: '#9ca3af' }] },
        options: { scales: { y: { beginAtZero: true } } } });
      document.getElementById('champions').innerHTML = table([
        { label: 'Season', value: 'label', left: true }, { label: 'Champion', value: r => `${r.champion} · ${r.champion_name}`, left: true },
        { label: 'Runner-up', value: r => r.runner_up || '–', left: true }, { label: 'Source', value: 'source', left: true }], d.champions.slice().reverse());
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
  }
  document.getElementById('reload').addEventListener('click', load);
  load();
})();
