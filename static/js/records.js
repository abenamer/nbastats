(async () => {
  const { num, esc, getJSON, table, setStatus, labelOf, RESULT } = NBA;
  const status = document.getElementById('status');
  const league = l => l === 'aba' ? 'ABA' : 'NBA';
  const team = e => `${e.code} · ${e.name}`;
  const rec = e => `${e.wins}-${e.losses}`;
  const entryCols = (valueLabel, withDate = true) => [
    { label: 'Season', value: e => labelOf(e.season), left: true }, { label: 'Team', value: team, left: true }, { label: 'Lg', value: e => league(e.league), left: true },
    { label: valueLabel, value: e => num(e.value, 0) }, ...(withDate ? [{ label: 'Date', value: e => e.date || '', left: true }] : []),
    { label: 'W-L', value: rec, left: true }, { label: 'Result', value: e => RESULT[e.playoff_result] || e.playoff_result, left: true, cls: e => e.playoff_result === 'champion' ? 'good' : '' }];
  const list = (id, rows, valueLabel, withDate) => { document.getElementById(id).innerHTML = rows.length ? table(entryCols(valueLabel, withDate), rows) : '<p class="muted">No data.</p>'; };
  const pair = (x, which) => `${x.extremes[which].code} ${num(x.extremes[which].value, 0)} (${labelOf(x.extremes[which].season)})`;
  const groupCols = [
    { label: 'Season', value: g => labelOf(g.season), left: true }, { label: 'Conference', value: g => g.conference === 'west' ? 'West' : 'East', left: true },
    { label: 'Division', value: g => g.division || '', left: true }, { label: 'Teams', value: 'teams' }, { label: 'Mean Elo', value: g => num(g.mean_elo, 0) },
    { label: 'Best', value: g => `${g.best_code} ${num(g.best_elo, 0)}`, left: true }, { label: 'Worst', value: g => `${g.worst_code} ${num(g.worst_elo, 0)}`, left: true }];
  const group = (id, rows) => { document.getElementById(id).innerHTML = rows.length ? table(groupCols, rows) : '<p class="muted">No alignment data.</p>'; };
  try {
    setStatus(status, 'Loading…');
    const r = await getJSON('/api/records');
    list('allTimeHigh', r.all_time_high, 'Peak Elo'); list('allTimeLow', r.all_time_low, 'Low Elo');
    document.getElementById('byDecade').innerHTML = table([
      { label: 'Decade', value: d => `${d.decade}s`, left: true }, { label: 'Team-seasons', value: 'teams' },
      { label: 'Highest', value: d => pair(d, 'best'), left: true, cls: () => 'good' }, { label: 'Lowest', value: d => pair(d, 'worst'), left: true, cls: () => 'bad' }], r.by_decade);
    const eraCols = [
      { label: 'Era', value: e => e.first_season === e.last_season ? labelOf(e.first_season) : `${labelOf(e.first_season)} to ${labelOf(e.last_season)}`, left: true },
      { label: 'Teams', value: 'teams' }, { label: 'Seasons', value: 'seasons' }, { label: '3-pt line', value: e => e.three_point ? 'yes' : 'no', left: true },
      { label: 'Highest', value: e => pair(e, 'best'), left: true, cls: () => 'good' }, { label: 'Lowest', value: e => pair(e, 'worst'), left: true, cls: () => 'bad' }];
    document.getElementById('byEraNba').innerHTML = table(eraCols, r.by_era.filter(e => e.league === 'nba'));
    document.getElementById('byEraAba').innerHTML = table(eraCols, r.by_era.filter(e => e.league === 'aba'));
    document.getElementById('byLeague').innerHTML = table([
      { label: 'League', value: l => league(l.league), left: true }, { label: 'Seasons', value: l => `${labelOf(l.first_season)} to ${labelOf(l.last_season)}`, left: true },
      { label: 'Highest', value: l => pair(l, 'best'), left: true, cls: () => 'good' }, { label: 'Lowest', value: l => pair(l, 'worst'), left: true, cls: () => 'bad' }], r.by_league);
    document.getElementById('bySize').innerHTML = table([
      { label: 'Teams', value: 'teams' }, { label: 'League', value: s => league(s.league), left: true }, { label: 'Seasons', value: s => `${s.seasons} (${labelOf(s.first_season)} to ${labelOf(s.last_season)})`, left: true },
      { label: 'Highest', value: s => pair(s, 'best'), left: true, cls: () => 'good' }, { label: 'Lowest', value: s => pair(s, 'worst'), left: true, cls: () => 'bad' }], r.by_league_size);
    document.getElementById('confSeasons').innerHTML = table([
      { label: 'Season', value: c => labelOf(c.season), left: true },
      { label: 'East mean', value: c => num(c.east_mean, 0), cls: c => c.gap < 0 ? 'good' : '' }, { label: 'West mean', value: c => num(c.west_mean, 0), cls: c => c.gap > 0 ? 'good' : '' },
      { label: 'Gap (W − E)', value: c => `${c.gap >= 0 ? '+' : ''}${num(c.gap, 0)}` }, { label: 'Stronger', value: c => Math.abs(c.gap) < 5 ? 'even' : (c.gap > 0 ? 'West' : 'East'), left: true },
      { label: 'East best', value: 'east_best_code', left: true }, { label: 'West best', value: 'west_best_code', left: true }], r.conference_seasons);
    group('divBest', r.divisions.best); group('divWorst', r.divisions.worst);
    list('bestChampion', r.best_champion, 'Peak Elo'); list('worstChampion', r.worst_champion, 'Peak Elo'); list('bestUncrowned', r.best_uncrowned, 'Peak Elo');
    list('bestEnd', r.best_end, 'End Elo', false); list('worstEnd', r.worst_end, 'End Elo', false);
    list('rise', r.biggest_rise, 'Change', false); list('fall', r.biggest_fall, 'Change', false);
    document.getElementById('swings').innerHTML = table([
      { label: 'Date', value: 'game_date', left: true }, { label: 'Season', value: s => labelOf(s.season), left: true }, { label: 'Winner', value: s => `${s.code} · ${s.name}`, left: true },
      { label: 'Score', value: s => `${s.points}-${s.opponent_points} ${s.home ? 'vs' : 'at'} ${s.opponent_code}${s.playoff ? ' (playoffs)' : ''}`, left: true },
      { label: 'Elo before', value: s => num(s.elo_pre, 0) }, { label: 'Opponent Elo', value: s => num(s.opponent_elo_pre, 0) }, { label: 'Gain', value: s => `+${num(s.elo_post - s.elo_pre, 1)}`, cls: () => 'good' }], r.swings);
    setStatus(status, '');
  } catch (e) { setStatus(status, e.message, true); }
})();
