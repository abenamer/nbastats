(async () => {
  const { num, esc, getJSON, table, setStatus, labelOf } = NBA;
  const status = document.getElementById('status');
  let rows = [], boxRows = [], boxByPlayer = new Map(), boxFailed = false;
  const signed = (v, d) => (v >= 0 ? '+' : '') + num(v, d);
  const boxCell = p => { const b = boxByPlayer.get(p.player_id); return b == null ? '' : signed(b, 0); };
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
      { label: 'Box (Elo)', value: boxCell, title: 'The same player from his box score: what the team gains over a replacement in the games he plays, in Elo, summed over his teams', cls: p => { const b = boxByPlayer.get(p.player_id); return b > 0 ? 'good' : (b < 0 ? 'bad' : ''); } },
      { label: 'JSON', value: p => `<a href="/api/players/${p.player_id}/worth" target="_blank">seasons</a>`, raw: true, left: true }], shown)
      + (!all && !q && rows.length > 40 ? `<p class="muted small">Top 40 of ${rows.length} players with an absence; tick <em>Every player</em> or filter.</p>` : '')
      : '<p class="muted small">No player with an absence and at least five games in this season: load its box scores on the Data page.</p>';
  }
  function renderBox(b) {
    const q = document.getElementById('filter').value.trim().toLowerCase();
    const all = document.getElementById('allRows').checked;
    let shown = boxRows.filter(p => !q || p.name.toLowerCase().includes(q) || p.team_code.toLowerCase().includes(q));
    if (!all && !q) shown = shown.slice(0, 40);
    const bpm = b && b.bpm_available;
    document.getElementById('box').innerHTML = shown.length ? table([
      { label: 'Player', value: p => p.name, left: true }, { label: 'Team', value: 'team_code', left: true },
      { label: 'Games', value: 'games' }, { label: 'Min %', value: p => num(100 * p.minutes_share, 0) },
      { label: 'Pos', value: p => num(p.position, 1), title: '1 point guard to 5 center, estimated from the box score' },
      { label: 'Role', value: p => num(p.offensive_role, 1), title: '1 creator to 5 receiver, estimated from the box score' },
      ...(bpm ? [
        { label: 'BPM', value: p => signed(p.bpm, 1), cls: p => p.bpm > 0 ? 'good' : (p.bpm < 0 ? 'bad' : '') },
        { label: 'OBPM', value: p => signed(p.obpm, 1) }, { label: 'DBPM', value: p => signed(p.dbpm, 1) },
        { label: 'VORP', value: p => num(p.vorp, 1) }, { label: 'Wins', value: p => num(p.wins, 1), title: 'VORP times 2.7' }] : []),
      { label: 'PER', value: p => p.per == null ? '' : num(p.per, 1) },
      ...(bpm ? [{ label: 'Box worth (Elo)', value: p => signed(p.worth_elo, 0), cls: p => p.worth_elo > 0 ? 'good' : (p.worth_elo < 0 ? 'bad' : ''), title: 'Points per game over a replacement player in the games he plays, times 28; not diluted by games missed' }] : [])], shown)
      + (!all && !q && boxRows.length > 40 ? `<p class="muted small">Top 40 of ${boxRows.length} player seasons; tick <em>Every player</em> or filter.</p>` : '')
      : ((b && b.note) || boxFailed ? '' : '<p class="muted small">No box scores with minutes for this season: load them on the Data page.</p>');
  }
  let boxTable = null;
  async function load() {
    const season = parseInt(document.getElementById('season').value);
    setStatus(status, 'loading…');
    try {
      // The box table is fetched on its own: a failure there must not blank the other panels.
      boxFailed = false;
      const boxPromise = getJSON(`/api/seasons/${season}/box-worth`).then(b => { boxTable = b; boxRows = b.players; boxByPlayer = new Map();
        if (b.bpm_available) for (const p of b.players) boxByPlayer.set(p.player_id, (boxByPlayer.get(p.player_id) || 0) + p.worth_elo);
        return b; }, e => { boxFailed = true; boxTable = null; boxRows = []; boxByPlayer = new Map(); document.getElementById('boxNote').textContent = `Box-score worth unavailable: ${e.message}`; document.getElementById('box').innerHTML = ''; return null; });
      const [w, a, b] = await Promise.all([getJSON(`/api/seasons/${season}/player-worth`), getJSON(`/api/seasons/${season}/availability`), boxPromise]);
      if (b) document.getElementById('boxNote').textContent = b.note ? b.note : b.games
        ? `${labelOf(season)}: ${b.games.toLocaleString()} regular-season games with box scores for both teams${b.games_without_lines ? ` (${b.games_without_lines} without)` : ''}; league pace ${num(b.league.pace, 1)} possessions per 48 minutes. BPM is points per 100 possessions above a league-average player, from the box score weighted by estimated position and role, with each team's players summing to its schedule-adjusted efficiency. VORP is BPM over a replacement level of ${num(b.replacement_level, 1)}, by minutes share and games. Box worth is BPM over replacement, by his share of the floor in the games he plays and the team's pace, on the Elo scale: what the box score says, where the Worth column is what the missed games say.${b.per_partial ? ' PER is computed with the missing statistics as zero.' : ''}`
        : '';
      if (b) renderBox(b);
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
    } catch (e) { setStatus(status, e.message, true); rows = []; boxRows = []; boxByPlayer = new Map(); renderWorth(); renderBox(null); }
  }
  document.getElementById('season').addEventListener('change', load);
  document.getElementById('filter').addEventListener('input', () => { renderWorth(); renderBox(boxTable); });
  document.getElementById('allRows').addEventListener('change', () => { renderWorth(); renderBox(boxTable); });
  load();
})();
