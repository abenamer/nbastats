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
        ? `Injury report of ${a.report_date}: ${lines.length - a.unlisted} players listed${a.unlisted ? ` and ${a.unlisted} more absent from the box scores without a line` : ''}, ${a.absences.length} with a penalty (a positive worth from this season or the two before). ${a.statuses && a.statuses.curves.length ? 'Each line is weighted by the measured chance that a line of its status, for a player not yet out or already in a run, misses the next game (see What a status has meant below); a status with too few lines keeps its default: out in full, doubtful three quarters, questionable or day-to-day half, probable a quarter.' : 'Out counts in full, doubtful three quarters, questionable or day-to-day half, probable a quarter.'} A return date beyond the team's next game stands. A return date that is the next game is the feed's placeholder: a player out, or in a run of three missed games, is then priced for the expected remaining games of a run as long as his${a.survival ? `, read from ${a.survival.runs.toLocaleString()} runs of ${a.survival.seasons.map(labelOf).join(', ')} (a new run lasts ${num(a.survival.mean_run_games, 1)} games on average; one already ten games old has ${num(a.survival.expected_remaining[10], 1)} to go)` : '; too few runs are stored for that curve, so the status defaults stand'}. Any other line whose status has a curve is priced for the games a line of its status has gone on to miss. Without a curve to lean on, a return date of today prices nothing; a worth at or below zero never does. A player worth within one error of zero gives way to his box-score worth.`
        : 'No injury report stored yet: fetch one on the Data page.';
      document.getElementById('availability').innerHTML = lines.length ? table([
        { label: 'Player', value: 'name', left: true }, { label: 'Team', value: 'team_code', left: true }, { label: 'Status', value: 'status_text', left: true },
        { label: 'Return', value: l => l.return_date || '', left: true },
        { label: 'Missed', value: 'missed_so_far', title: 'Team games missed in a row up to the latest game with box scores' },
        { label: 'Worth (Elo)', value: l => l.worth_elo == null ? 'no estimate' : `${signed(l.worth_elo, 0)}${l.se_elo ? ` ± ${num(l.se_elo, 0)}` : ''} (${labelOf(l.worth_season)}, ${l.worth_source === 'box' ? 'box score' : 'missed games'})`, left: true, title: 'The estimate the line is priced with: the worth from the missed games with its error, or the box-score worth when the first is missing or within one error of zero' },
        { label: 'Weight', value: l => l.weight == null ? '' : `${num(100 * l.weight, 0)}% (${l.weight_source === 'measured' ? 'measured' : 'default'})`, left: true, title: 'The share of the worth taken off: the measured chance that a line of this status, in this state, misses the next game, or the status default' },
        { label: 'Penalty', value: l => l.penalty_elo == null ? '' : `${num(l.penalty_elo, 0)} to ${l.until} (${{ return_date: 'return date', expected: 'expected run', measured: 'status curve', status: 'status default', season: 'season' }[l.span] || l.span})`, left: true, cls: l => l.penalty_elo ? 'bad' : '', title: 'The penalty and the last date it applies, with what set that date' }], lines) : '';
      renderStatuses(a.statuses);
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); rows = []; boxRows = []; boxByPlayer = new Map(); renderWorth(); renderBox(null); }
  }
  // What a status has meant: the measured weight and span behind each status.
  const statusNames = { out: 'Out', out_for_season: 'Out for season', absent: 'Absent', doubtful: 'Doubtful', questionable: 'Questionable', day_to_day: 'Day-to-day', probable: 'Probable', available: 'Available', unknown: 'Unknown' };
  function renderStatuses(t) {
    document.getElementById('statusNote').textContent = t
      ? `${t.lines.toLocaleString()} report lines of ${t.seasons.map(labelOf).join(', ')} followed through the box scores: did the player miss his team's next game, and how many of the next thirty. A line is read in the player's state on the report day: not yet out (he played the team's last game before it) or already in a run (he had missed it). A status needs 50 lines in a state for a curve; below that its default weight and span stand. The weight is the share missing the next game. The span is the games missed over thirty divided by the weight, so weight times span is the games missed. A player who left the team reads as missing every game, so the long end of a curve runs high.`
      : 'No stored report line could be followed through the box scores yet: the default weights and spans stand. The curves fill in as the daily reports and box scores accumulate.';
    document.getElementById('statusCurves').innerHTML = t && t.curves.length ? table([
      { label: 'Status', value: c => statusNames[c.status] || c.status, left: true },
      { label: 'State', value: c => c.in_run ? 'already in a run' : 'not yet out', left: true, title: 'Not yet out: he played the team\'s last game before the report. Already in a run: he had missed it' },
      { label: 'Lines', value: 'samples', title: 'Report lines followed' },
      { label: 'Misses tonight', value: c => `${num(100 * c.weight, 0)}%`, title: 'The share of the lines whose player missed the next game: the weight used' },
      { label: 'Default weight', value: c => `${num(100 * c.default_weight, 0)}%`, title: 'The weight a status without a curve gets' },
      { label: 'Games missed of 30', value: c => num(c.expected_missed, 1), title: 'Of the team\'s next thirty games, in expectation' },
      { label: 'Span (games)', value: c => num(c.span_games, 1), title: 'Games missed divided by the weight: the games the weight is applied over' },
      { label: 'Default span', value: c => `${c.default_days} d`, title: 'The span in days a status without a curve gets' },
      { label: 'Still out after 1, 5, 10', value: c => [1, 5, 10].map(j => c.miss[j] == null ? '–' : `${num(100 * c.miss[j], 0)}%`).join(' / '), left: true, title: 'The share of the lines whose player missed the team\'s second, sixth and eleventh game from the report' }], t.curves) : '';
  }
  // Return from absence: a report over a range of seasons, run on request.
  const returnStatus = document.getElementById('returnStatus');
  const seasonOptions = Array.from(document.getElementById('season').options).map(o => parseInt(o.value));
  const latestSeason = seasonOptions.length ? Math.max(...seasonOptions) : new Date().getFullYear();
  document.getElementById('returnTo').value = latestSeason;
  document.getElementById('returnFrom').value = latestSeason - 9;
  const opt = (v, d) => (v == null ? '' : num(v, d));
  // A value that rounds to zero reads as 0, not -0.
  const flat = (v, d) => (Math.abs(v) < 0.5 * 10 ** -d ? 0 : v);
  const optSigned = (v, d) => (v == null ? '' : signed(flat(v, d), d));
  const withSe = (v, se, d) => (v == null ? '' : `${signed(flat(v, d), d)}${se == null ? '' : ` ± ${num(se, d)}`}`);
  const dropCls = v => (v == null ? '' : v < 0 ? 'bad' : v > 0 ? 'good' : '');
  function renderReturn(r) {
    const seasonsRead = r.seasons.length;
    const skipped = r.skipped.length ? ` ${r.skipped.length} season${r.skipped.length > 1 ? 's' : ''} skipped (${r.skipped.slice(0, 4).map(s => `${labelOf(s.season)}: ${s.reason}`).join('; ')}${r.skipped.length > 4 ? '; …' : ''}).` : '';
    document.getElementById('returnNote').textContent = r.note
      ? `${r.note}${skipped}`
      : `${labelOf(r.from)} to ${labelOf(r.to)}: ${seasonsRead} season${seasonsRead > 1 ? 's' : ''} read, ${r.events_total.toLocaleString()} return events, ${r.controls_total.toLocaleString()} control samples.${skipped} Worth is the box worth per game played, in Elo (${num(r.parameters.elo_per_point, 0)} per point of margin). Drop is the worth after the return minus the worth before, averaged over the events, with the standard error of that mean. Control drop is the same drop over every clean forty-game stretch of a regular player in the same bracket: what two windows show without an injury. Effect is the drop minus the control drop; its error combines both. A group needs many events before a difference of a few Elo means anything.`;
    const group = c => c.bracket == null && c.length == null ? 'All events' : c.bracket == null ? c.length : c.length == null ? c.bracket : `${c.bracket}, ${c.length}`;
    document.getElementById('returnCells').innerHTML = r.cells.length ? table([
      { label: 'Group', value: group, left: true },
      { label: 'Events', value: 'events' },
      { label: 'Worth before', value: c => optSigned(c.pre_worth, 0), title: 'Mean box worth per game played in the twenty team games before the run, Elo' },
      { label: 'First 10 back', value: c => optSigned(c.early_worth, 0), title: 'Mean box worth per game played in the first ten team games from the return' },
      { label: 'Next 10', value: c => optSigned(c.late_worth, 0), title: 'Mean box worth per game played in the next ten team games, over the events that had them' },
      { label: 'Drop, first 10', value: c => withSe(c.drop_early, c.drop_early_se, 0), cls: c => dropCls(c.drop_early), title: 'Mean of (first ten back minus before), with its standard error' },
      { label: 'Drop, next 10', value: c => optSigned(c.drop_late, 0), cls: c => dropCls(c.drop_late) },
      { label: 'Controls', value: 'controls', title: 'Control samples in the brackets of the group\'s events: clean forty-game stretches of regular players' },
      { label: 'Control drop', value: c => optSigned(c.control_drop_early, 0), title: 'The same drop over the control samples, each event taking its bracket\'s mean: what two windows show without an injury' },
      { label: 'Effect, first 10', value: c => withSe(c.effect_early, c.effect_early_se, 0), cls: c => dropCls(c.effect_early), title: 'Drop minus control drop, with the error of both' },
      { label: 'Effect, next 10', value: c => optSigned(c.effect_late, 0), cls: c => dropCls(c.effect_late) },
      { label: 'BPM before', value: c => optSigned(c.pre_bpm, 1) },
      { label: 'BPM first 10', value: c => optSigned(c.early_bpm, 1) },
      { label: 'BPM next 10', value: c => optSigned(c.late_bpm, 1) },
      { label: 'Min before', value: c => opt(c.pre_minutes, 1), title: 'Minutes per game played' },
      { label: 'Min first 10', value: c => opt(c.early_minutes, 1) },
      { label: 'Min next 10', value: c => opt(c.late_minutes, 1) },
      { label: 'Played, first 10', value: c => c.early_availability == null ? '' : num(100 * c.early_availability, 0) + '%', title: 'Games played over team games in the window' },
      { label: 'Played, next 10', value: c => c.late_availability == null ? '' : num(100 * c.late_availability, 0) + '%' }], r.cells) : '';
    document.getElementById('returnEventRows').innerHTML = r.events ? (r.events.length ? '<h3>Events</h3>' + table([
      { label: 'Season', value: e => labelOf(e.season), left: true }, { label: 'Player', value: 'name', left: true }, { label: 'Team', value: 'team_code', left: true },
      { label: 'Missed', value: 'missed' }, { label: 'From', value: 'first_missed', left: true }, { label: 'Back', value: 'returned', left: true },
      { label: 'Bracket', value: 'bracket', left: true }, { label: 'Length', value: 'length', left: true },
      { label: 'Worth before', value: e => optSigned(e.pre.worth_elo, 0) }, { label: 'First 10 back', value: e => optSigned(e.early.worth_elo, 0) },
      { label: 'Next 10', value: e => e.late ? optSigned(e.late.worth_elo, 0) : '' },
      { label: 'Min before', value: e => num(e.pre.minutes_per_game, 1) }, { label: 'Min first 10', value: e => num(e.early.minutes_per_game, 1) },
      { label: 'Played, first 10', value: e => `${e.early.played}/${e.early.team_games}` }, { label: 'Played, next 10', value: e => e.late ? `${e.late.played}/${e.late.team_games}` : '' }], r.events) : '') : '';
  }
  async function runReturn() {
    const from = parseInt(document.getElementById('returnFrom').value), to = parseInt(document.getElementById('returnTo').value);
    if (!Number.isInteger(from) || !Number.isInteger(to)) { setStatus(returnStatus, 'enter two season end years', true); return; }
    if (from > to) { setStatus(returnStatus, 'from must not be after to', true); return; }
    const events = document.getElementById('returnEvents').checked;
    setStatus(returnStatus, 'reading the seasons… (the first run of a range computes every season\'s box table)');
    document.getElementById('returnRun').disabled = true;
    try {
      renderReturn(await getJSON(`/api/research/return-curve?from=${from}&to=${to}&events=${events}`));
      setStatus(returnStatus, '');
    } catch (e) { setStatus(returnStatus, e.message, true); }
    document.getElementById('returnRun').disabled = false;
  }
  document.getElementById('returnRun').addEventListener('click', runReturn);
  document.getElementById('season').addEventListener('change', load);
  document.getElementById('filter').addEventListener('input', () => { renderWorth(); renderBox(boxTable); });
  document.getElementById('allRows').addEventListener('change', () => { renderWorth(); renderBox(boxTable); });
  load();
})();
