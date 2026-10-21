(async () => {
  const { pct, num, esc, getJSON, table, setStatus, chart, color, RESULT } = NBA;
  const status = document.getElementById('status');
  async function loadSeason() {
    const season = document.getElementById('season').value;
    const teams = document.getElementById('teams').value.trim();
    setStatus(status, 'Loading…');
    try {
      const [t, h] = await Promise.all([getJSON(`/api/seasons/${season}/teams`), getJSON(`/api/seasons/${season}/elo-history?teams=${encodeURIComponent(teams)}`)]);
      document.getElementById('ratings').innerHTML = table([
        { label: 'Team', value: r => `${r.code} · ${r.team_name}`, left: true }, { label: 'Conf', value: r => r.conference || '', left: true },
        { label: 'W', value: 'wins' }, { label: 'L', value: 'losses' }, { label: 'Start', value: r => num(r.elo_start, 0) },
        { label: 'Reg. end', value: r => num(r.elo_regular_end, 0) }, { label: 'Pre-playoffs', value: r => num(r.elo_pre_playoffs, 0) },
        { label: 'End', value: r => num(r.elo_end, 0) }, { label: 'Peak', value: r => num(r.elo_peak, 0) }, { label: 'Seed', value: r => r.seed || '' },
        { label: 'Result', value: r => RESULT[r.playoff_result] || r.playoff_result, left: true, cls: r => r.playoff_result === 'champion' ? 'good' : '' }],
        t.teams.slice().sort((a, b) => (b.elo_pre_playoffs || 0) - (a.elo_pre_playoffs || 0)));
      const sel = teams ? h.series : h.series;
      chart('historyChart', { type: 'line', data: { datasets: sel.map((s, i) => ({ label: s.code, data: s.points.map(p => ({ x: p[0], y: p[1] })), borderColor: color(i), borderWidth: teams ? 2 : 1, pointRadius: 0, tension: .2 })) },
        options: { parsing: false, interaction: { mode: 'nearest', intersect: false }, plugins: { legend: { display: sel.length <= 12 } }, scales: { x: { type: 'category', ticks: { maxTicksLimit: 10 } } } } });
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
  }
  async function loadFranchise() {
    const key = document.getElementById('franchise').value, from = document.getElementById('fFrom').value, to = document.getElementById('fTo').value;
    try {
      const d = await getJSON(`/api/franchises/${key}/elo?from=${from}&to=${to}`);
      chart('franchiseChart', { type: 'line', data: { datasets: [{ label: `${d.franchise.name} Elo after each game`, data: d.points.map(p => ({ x: p.game_date, y: p.elo_post })), borderColor: '#2563eb', pointRadius: 0, borderWidth: 1 }] },
        options: { parsing: false, plugins: { legend: { display: true } }, scales: { x: { type: 'category', ticks: { maxTicksLimit: 12 } } } } });
      document.getElementById('franchiseSeasons').innerHTML = table([
        { label: 'Season', value: r => NBA.labelOf(r.season), left: true }, { label: 'Code', value: 'code', left: true }, { label: 'W', value: 'wins' }, { label: 'L', value: 'losses' },
        { label: 'Pre-playoffs Elo', value: r => num(r.elo_pre_playoffs, 0) }, { label: 'Peak', value: r => num(r.elo_peak, 0) }, { label: 'Seed', value: r => r.seed || '' },
        { label: 'Result', value: r => RESULT[r.playoff_result] || r.playoff_result, left: true, cls: r => r.playoff_result === 'champion' ? 'good' : '' }], d.seasons);
    } catch (e) { setStatus(status, e.message, true); }
  }
  async function calc() {
    const q = new URLSearchParams({ elo_a: document.getElementById('eloA').value, elo_b: document.getElementById('eloB').value, home: document.getElementById('home').value, home_advantage: document.getElementById('adv').value });
    try {
      const d = await getJSON(`/api/matchup?${q}`);
      document.getElementById('matchup').innerHTML = `<div>Elo difference (A − B)</div><div>${num(d.elo_diff, 0)}</div><div>P(A wins one game)</div><div><strong>${pct(d.p_game, 2)}</strong></div><div>P(A wins on a neutral floor)</div><div>${pct(d.p_game_neutral, 2)}</div><div>P(A wins best-of-7)</div><div><strong>${pct(d.p_series_best_of_7, 2)}</strong></div><div>P(A wins best-of-5)</div><div>${pct(d.p_series_best_of_5, 2)}</div>`;
    } catch (e) { setStatus(status, e.message, true); }
  }
  document.getElementById('load').addEventListener('click', loadSeason);
  document.getElementById('loadFranchise').addEventListener('click', loadFranchise);
  document.getElementById('calc').addEventListener('click', calc);
  loadSeason(); loadFranchise(); calc();
})();
