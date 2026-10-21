(async () => {
  const { pct, num, esc, runJob, table, setStatus, chart, RESULT } = NBA;
  const status = document.getElementById('status');
  document.getElementById('preseason').addEventListener('change', e => document.getElementById('asOf').disabled = e.target.checked);
  document.getElementById('asOf').disabled = true;
  document.getElementById('run').addEventListener('click', async () => {
    const btn = document.getElementById('run'); btn.disabled = true;
    const pk = document.getElementById('pk').value;
    const p = { season: parseInt(document.getElementById('season').value), iterations: parseInt(document.getElementById('iterations').value), seed: parseInt(document.getElementById('seed').value) || 0,
      home_advantage: parseFloat(document.getElementById('adv').value), regular_season_k: parseFloat(document.getElementById('k').value), playoff_k: pk === '' ? null : parseFloat(pk), playoff_scale: 1.0 };
    if (!document.getElementById('preseason').checked && document.getElementById('asOf').value) p.as_of = document.getElementById('asOf').value;
    const name = document.getElementById('name').value.trim(); if (name) p.name = name;
    try {
      const job = await runJob('/api/jobs/season', p, j => setStatus(status, `${j.status} ${pct(j.progress, 0)}`));
      const r = job.result;
      document.getElementById('note').textContent = `${r.information_cutoff_note} Games used: ${r.games_played}; regular-season games simulated: ${r.games_remaining}; qualification format: ${r.qualification}. ${r.iterations.toLocaleString()} simulated seasons in ${r.elapsed_ms} ms.`;
      const cols = [
        { label: 'Team', value: t => `${t.code} · ${t.name}`, left: true }, { label: 'Conf', value: 'conference', left: true }, { label: 'Elo', value: t => num(t.elo_as_of, 0) },
        { label: 'Record', value: t => `${t.wins}-${t.losses}` }, { label: 'Exp. wins', value: t => num(t.expected_wins, 1) },
        { label: 'Top 6', value: t => pct(t.p_top6) }, { label: 'Play-in', value: t => pct(t.p_play_in) }, { label: 'Playoffs', value: t => pct(t.p_playoffs) },
        { label: '2nd round', value: t => pct(t.p_second_round) }, { label: 'Conf. finals', value: t => pct(t.p_conf_finals) }, { label: 'Finals', value: t => pct(t.p_finals) },
        { label: 'Title', value: t => `${pct(t.p_champion, 2)} ± ${pct(1.96 * t.p_champion_se, 2)}`, cls: () => 'good' },
        { label: 'Actual', value: t => t.actual_result ? RESULT[t.actual_result] + (t.actual_seed ? ` (${t.actual_seed})` : '') : '–', left: true, cls: t => t.actual_result === 'champion' ? 'good' : '' }];
      document.getElementById('results').innerHTML = table(cols, r.teams);
      const top = r.teams.slice(0, 16);
      chart('champChart', { type: 'bar', data: { labels: top.map(t => t.code), datasets: [{ label: 'P(champion)', data: top.map(t => t.p_champion), backgroundColor: '#2563eb' }] }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      const all = r.teams.slice().sort((a, b) => b.p_playoffs - a.p_playoffs);
      chart('stageChart', { type: 'bar', data: { labels: all.map(t => t.code), datasets: [{ label: 'Playoffs', data: all.map(t => t.p_playoffs), backgroundColor: '#2563eb' }, { label: 'Play-in', data: all.map(t => t.p_play_in), backgroundColor: '#f59e0b' }, { label: 'Conf. finals', data: all.map(t => t.p_conf_finals), backgroundColor: '#16a34a' }] }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) }, max: 1 } } } });
      setStatus(status, `done · ${job.model_version}`);
    } catch (e) { setStatus(status, e.message, true); } finally { btn.disabled = false; }
  });
})();
