(async () => {
  const { num, esc, getJSON, table, setStatus, labelOf } = NBA;
  const status = document.getElementById('status');
  let rows = [];
  const signed = (v, d) => (v >= 0 ? '+' : '') + num(v, d);
  function renderWorth() {
    const q = document.getElementById('filter').value.trim().toLowerCase();
    const all = document.getElementById('allRows').checked;
    let shown = rows.filter(p => !q || p.name.toLowerCase().includes(q) || p.team_codes.some(c => c.toLowerCase().includes(q)));
    if (!all && !q) shown = shown.slice(0, 40);
    document.getElementById('worth').innerHTML = shown.length ? table([
      { label: 'Player', value: p => p.name, left: true }, { label: 'Team', value: p => p.team_codes.join(', '), left: true },
      { label: 'Games', value: 'games_played' }, { label: 'Missed', value: 'absences' },
      { label: 'Worth (Elo)', value: p => signed(p.worth_elo, 0), cls: p => p.worth_elo > 0 ? 'good' : (p.worth_elo < 0 ? 'bad' : '') },
      { label: '± SE', value: p => num(p.se_elo, 0) },
      { label: 'Points / game', value: p => signed(p.worth_points, 1) },
      { label: 'With', value: p => signed(p.with_mean, 1), title: 'Mean residual of the team in the games he played' },
      { label: 'Without', value: p => signed(p.without_mean, 1), title: 'Mean residual of the team in the games he missed' },
      { label: 'JSON', value: p => `<a href="/api/players/${p.player_id}/worth" target="_blank">seasons</a>`, raw: true, left: true }], shown)
      + (!all && !q && rows.length > 40 ? `<p class="muted small">Top 40 of ${rows.length} players with an absence; tick <em>Every player</em> or filter.</p>` : '')
      : '<p class="muted small">No player with an absence and at least five games in this season: load its box scores on the Data page.</p>';
  }
  async function load() {
    const season = parseInt(document.getElementById('season').value);
    setStatus(status, 'loading…');
    try {
      const [w, a] = await Promise.all([getJSON(`/api/seasons/${season}/player-worth`), getJSON(`/api/seasons/${season}/availability`)]);
      rows = w.players;
      document.getElementById('worthNote').textContent = `${labelOf(season)}: ${w.games.toLocaleString()} played games, ${w.games_with_absence.toLocaleString()} with an absence, ${w.players_seen.toLocaleString()} players seen, ${w.players.length} with an estimate. Residual spread ${num(w.residual_sd_points, 1)} points a game; prior spread of a worth ${num(w.prior_sd_points, 1)} points (${w.prior_estimated ? 'estimated from the raw with/without differences' : 'the default: too few players with five absences to estimate it'}); ${num(w.elo_per_point, 0)} Elo per point. A worth is what the team loses per game without the player; two players out together share the gap, and a handful of absences stay near zero.`;
      renderWorth();
      const lines = a.lines;
      document.getElementById('availabilityNote').textContent = a.note
        ? a.note
        : a.report_date
        ? `Injury report of ${a.report_date}: ${lines.length} players listed, ${a.absences.length} with a penalty (a positive worth from this season or the two before). Out counts in full, doubtful three quarters, questionable or day-to-day half, probable a quarter, until the day before the return date or for a few days when none is given.`
        : 'No injury report stored yet: fetch one on the Data page.';
      document.getElementById('availability').innerHTML = lines.length ? table([
        { label: 'Player', value: 'name', left: true }, { label: 'Team', value: 'team_code', left: true }, { label: 'Status', value: 'status_text', left: true },
        { label: 'Return', value: l => l.return_date || '', left: true },
        { label: 'Worth (Elo)', value: l => l.worth_elo == null ? 'no estimate' : `${signed(l.worth_elo, 0)} (${labelOf(l.worth_season)})`, left: true },
        { label: 'Penalty', value: l => l.penalty_elo == null ? '' : `${num(l.penalty_elo, 0)} to ${l.until}`, left: true, cls: l => l.penalty_elo ? 'bad' : '' }], lines) : '';
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); rows = []; renderWorth(); }
  }
  document.getElementById('season').addEventListener('change', load);
  document.getElementById('filter').addEventListener('input', renderWorth);
  document.getElementById('allRows').addEventListener('change', renderWorth);
  load();
})();
