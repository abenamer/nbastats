(async () => {
  const { pct, num, esc, getJSON, table, setStatus, chart, color, RESULT, dayOf, dateOf, dateScale, labelOf, unplayedShade } = NBA;
  // Tooltip for a rating series: date, team, Elo after the game and the change from the previous game.
  const eloTooltip = {
    title: items => items.length ? dateOf(items[0].raw.x) : '',
    label: item => { const ds = item.dataset, p = item.raw, prev = ds.data[item.dataIndex - 1]; const delta = prev ? p.y - prev.y : null;
      return `${ds.label}: ${num(p.y, 0)}${delta == null ? ' (first game)' : ` (${delta >= 0 ? '+' : ''}${num(delta, 1)})`}${p.type === 'playoff' ? ' · playoffs' : p.type === 'play_in' ? ' · play-in' : ''}`; } };
  const status = document.getElementById('status');
  async function loadSeason() {
    const season = document.getElementById('season').value;
    const teams = document.getElementById('teams').value.trim();
    setStatus(status, 'Loading…');
    try {
      const [t, h] = await Promise.all([getJSON(`/api/seasons/${season}/teams`), getJSON(`/api/seasons/${season}/elo-history?teams=${encodeURIComponent(teams)}`)]);
      document.getElementById('ratings').innerHTML = table([
        // The code only, so every column fits; the name is the tooltip. A click toggles the team in the highlight field.
        { label: 'Team', value: r => `<span class="team-pick" data-code="${esc(r.code)}" title="${esc(r.team_name)} · click to highlight">${esc(r.code)}</span>`, raw: true, left: true }, { label: 'Conf', value: r => r.conference || '', left: true },
        { label: 'W', value: 'wins' }, { label: 'L', value: 'losses' }, { label: 'Start', value: r => num(r.elo_start, 0) },
        { label: 'Reg. end', value: r => num(r.elo_regular_end, 0) }, { label: 'Pre-playoffs', value: r => num(r.elo_pre_playoffs, 0) },
        { label: 'End', value: r => num(r.elo_end, 0) }, { label: 'Peak', value: r => num(r.elo_peak, 0) }, { label: 'Seed', value: r => r.seed || '' },
        { label: 'Result', value: r => RESULT[r.playoff_result] || r.playoff_result, left: true, cls: r => r.playoff_result === 'champion' ? 'good' : '' }],
        t.teams.slice().sort((a, b) => (b.elo_pre_playoffs || 0) - (a.elo_pre_playoffs || 0)));
      const sel = h.series, sched = h.schedule || {};
      // The axis covers the played games. When games remain, a shaded strip on the right stands
      // for the rest of the schedule (not to scale), so a young season still reads as lines.
      const days = sel.flatMap(s => s.points.map(p => dayOf(p[0])));
      const rest = sched.remaining > 0 && sched.last_played && days.length;
      const cut = rest ? dayOf(sched.last_played) + 0.5 : null;
      const strip = rest ? Math.max((Math.max(...days) - Math.min(...days)) * 0.35, 3) : 0;
      const shade = rest ? { from: cut, label: `Not played yet · ${num(sched.remaining, 0)} games to ${new Date(dayOf(sched.last) * 86400000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' })}` } : {};
      // A team with one game is a dot; with two or more it is a line.
      const radius = s => s.points.length === 1 ? 4 : teams ? 2 : 0;
      chart('historyChart', { type: 'line', plugins: [unplayedShade], data: { datasets: sel.map((s, i) => ({ label: s.code, data: s.points.map(p => ({ x: dayOf(p[0]), y: p[1], type: p[2] })), borderColor: color(i), backgroundColor: color(i), borderWidth: teams ? 2 : 1.2, pointRadius: radius(s), pointHitRadius: 6, tension: 0 })) },
        options: { responsive: true, maintainAspectRatio: false, parsing: false, animation: false, interaction: { mode: 'nearest', axis: 'x', intersect: false }, plugins: { legend: { display: sel.length <= 12 }, tooltip: { callbacks: eloTooltip }, unplayedShade: shade },
          scales: { x: dateScale(days, rest ? { max: cut + strip } : {}, cut), y: { title: { display: true, text: 'Elo after the game' } } } } });
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
  }
  async function loadFranchise() {
    const key = document.getElementById('franchise').value, from = document.getElementById('fFrom').value, to = document.getElementById('fTo').value;
    try {
      const d = await getJSON(`/api/franchises/${key}/elo?from=${from}&to=${to}`);
      const pts = d.points.map(p => ({ x: dayOf(p.game_date), y: p.elo_post, type: p.game_type, season: p.season }));
      const c = d.colors || { primary: '#2563eb', secondary: '#cbd5e1' };
      const lum = h => { const v = [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16) / 255).map(v => v <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4); return .2126 * v[0] + .7152 * v[1] + .0722 * v[2]; };
      const [line, area] = lum(c.primary) <= lum(c.secondary) ? [c.primary, c.secondary] : [c.secondary, c.primary];
      const rgba = (h, a) => `rgba(${[1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16)).join(',')},${a})`;
      chart('franchiseChart', { type: 'line', data: { datasets: [
          { label: d.franchise.name, data: pts, borderColor: line, backgroundColor: rgba(area, 0.28), fill: 'origin', pointRadius: 0, pointHitRadius: 6, borderWidth: 1.3 }] },
        options: { responsive: true, maintainAspectRatio: false, parsing: false, animation: false, interaction: { mode: 'nearest', axis: 'x', intersect: false },
          plugins: { legend: { display: true }, decimation: { enabled: true, algorithm: 'lttb', samples: 1500 },
            tooltip: { callbacks: { ...eloTooltip, label: item => `${eloTooltip.label(item)} · ${labelOf(item.raw.season)}` } } },
          scales: { x: dateScale(pts.map(p => p.x)), y: { title: { display: true, text: 'Elo after the game' } } } } });
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
  // A click on a team code in the ratings table adds it to (or removes it from) the highlight field and reloads.
  document.getElementById('ratings').addEventListener('click', e => {
    const pick = e.target.closest('.team-pick'); if (!pick) return;
    const input = document.getElementById('teams'), code = pick.dataset.code;
    const codes = input.value.split(',').map(s => s.trim().toUpperCase()).filter(Boolean);
    input.value = (codes.includes(code) ? codes.filter(c => c !== code) : codes.concat(code)).join(',');
    loadSeason();
  });
  document.getElementById('loadFranchise').addEventListener('click', loadFranchise);
  document.getElementById('calc').addEventListener('click', calc);
  loadSeason(); loadFranchise(); calc();
})();
