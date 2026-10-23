(async () => {
  const { num, esc, getJSON, table, setStatus, labelOf, RESULT } = NBA;
  const status = document.getElementById('status');
  const league = l => l === 'aba' ? 'ABA' : 'NBA';
  const team = e => `${e.code} · ${e.name}`;
  const rec = e => `${e.wins}-${e.losses}`;
  // The winner of a game and its margin, e.g. "NYK by 10"; a tie (none in the log) reads "tie".
  const winner = (home, hs, away, as_) => hs === as_ ? 'tie' : hs > as_ ? `${home} by ${hs - as_}` : `${away} by ${as_ - hs}`;
  // `endLabel` adds the rating after the team's last game (for a champion, the clinching game).
  const entryCols = (valueLabel, withDate = true, endLabel = null) => [
    { label: 'Season', value: e => labelOf(e.season), left: true }, { label: 'Team', value: team, left: true }, { label: 'Lg', value: e => league(e.league), left: true },
    { label: valueLabel, value: e => num(e.value, 0) }, ...(withDate ? [{ label: 'Date', value: e => e.date || '', left: true }] : []),
    ...(endLabel ? [{ label: endLabel, value: e => num(e.elo_end, 0) }] : []),
    { label: 'W-L', value: rec, left: true }, { label: 'Result', value: e => RESULT[e.playoff_result] || e.playoff_result, left: true, cls: e => e.playoff_result === 'champion' ? 'good' : '' }];
  const list = (id, rows, valueLabel, withDate, endLabel) => { document.getElementById(id).innerHTML = rows.length ? table(entryCols(valueLabel, withDate, endLabel), rows) : '<p class="muted">No data.</p>'; };
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
    const span = e => e.first_season === e.last_season ? labelOf(e.first_season) : `${labelOf(e.first_season)} to ${labelOf(e.last_season)}`;
    document.getElementById('periods').innerHTML = table([
      { label: 'League', value: p => league(p.league), left: true }, { label: 'Period', value: 'name', left: true }, { label: 'Seasons', value: span, left: true },
      { label: 'Eras', value: 'eras' }, { label: 'Rule change at the start', value: 'rule', left: true }], r.periods);
    const eraCols = [
      { label: 'Period', value: 'period', left: true }, { label: 'Era', value: span, left: true },
      { label: 'Teams', value: 'teams' }, { label: 'Seasons', value: 'seasons' },
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
      { label: 'East best', value: 'east_best_code', left: true }, { label: 'West best', value: 'west_best_code', left: true },
      { label: 'Title', value: c => c.champion_code ? `${c.champion_conference === 'west' ? 'West' : c.champion_conference === 'east' ? 'East' : '?'} · ${c.champion_code}` : '', left: true, cls: c => c.champion_conference && Math.abs(c.gap) >= 5 ? (c.champion_conference === 'west') === (c.gap > 0) ? 'good' : 'bad' : '' }], r.conference_seasons);
    // The top four winners, the rest as a count; the full list is the cell's tooltip.
    const titleList = c => { if (!c.champions.length) return 'none recorded'; const all = c.champions.map(t => `${t.code} ${t.titles}`); const shown = all.slice(0, 4).join(', '); return `<span title="${esc(all.join(', '))}">${esc(shown)}${all.length > 4 ? ` +${all.length - 4} more` : ''}</span>`; };
    document.getElementById('byCba').innerHTML = table([
      { label: 'Period', value: c => `<span title="${esc(c.summary)}">${esc(c.name)}</span>`, raw: true, left: true }, { label: 'Seasons', value: c => `${c.seasons} (${span(c)})`, left: true },
      { label: 'Best team-season', value: c => `${c.best.code} ${num(c.best.value, 0)} (${labelOf(c.best.season)})`, left: true, cls: () => 'good' },
      { label: 'Weakest', value: c => `${c.weakest.code} ${num(c.weakest.value, 0)} (${labelOf(c.weakest.season)})`, left: true, cls: () => 'bad' },
      { label: 'Titles', value: 'titles' }, { label: 'Champions', value: titleList, raw: true, left: true }, { label: 'Distinct', value: 'distinct_champions' },
      { label: 'Title HHI', value: c => c.title_hhi == null ? '–' : num(c.title_hhi, 2) }, { label: 'Elo spread', value: c => c.elo_spread == null ? '–' : num(c.elo_spread, 0) }], r.by_cba);
    document.getElementById('cbaChanges').innerHTML = r.by_cba.map(c => `<details><summary><strong>${esc(c.name)}</strong> · ${esc(span(c))} <span class="muted small">${esc(c.summary)}</span></summary><ul class="small">${c.changes.map(ch => `<li><span class="muted">${labelOf(ch.season)}</span> ${esc(ch.text)}</li>`).join('')}</ul></details>`).join('');
    group('divBest', r.divisions.best); group('divWorst', r.divisions.worst);
    list('bestChampion', r.best_champion, 'Peak Elo', true, 'At the clinch'); list('worstChampion', r.worst_champion, 'Peak Elo', true, 'At the clinch'); list('bestUncrowned', r.best_uncrowned, 'Peak Elo', true, 'Season end');
    list('bestEnd', r.best_end, 'End Elo', false); list('worstEnd', r.worst_end, 'End Elo', false);
    list('rise', r.biggest_rise, 'Change', false); list('fall', r.biggest_fall, 'Change', false);
    document.getElementById('swings').innerHTML = table([
      { label: 'Date', value: 'game_date', left: true }, { label: 'Season', value: s => labelOf(s.season), left: true }, { label: 'Winner', value: s => `${s.code} · ${s.name}`, left: true },
      { label: 'Score', value: s => `${s.points}-${s.opponent_points} ${s.home ? 'vs' : 'at'} ${s.opponent_code}${s.playoff ? ' (playoffs)' : ''}`, left: true },
      { label: 'Elo before', value: s => num(s.elo_pre, 0) }, { label: 'Opponent Elo', value: s => num(s.opponent_elo_pre, 0) }, { label: 'Gain', value: s => `+${num(s.elo_post - s.elo_pre, 1)}`, cls: () => 'good' }], r.swings);
    const matchups = (id, rows) => { document.getElementById(id).innerHTML = table([
      { label: 'Date', value: 'game_date', left: true }, { label: 'Season', value: m => labelOf(m.season), left: true },
      { label: 'Home', value: m => m.home_code, left: true }, { label: 'Elo', value: m => num(m.home_elo_pre, 0) },
      { label: 'Away', value: m => m.away_code, left: true }, { label: 'Elo', value: m => num(m.away_elo_pre, 0) },
      { label: 'Combined', value: m => num(m.home_elo_pre + m.away_elo_pre, 0) },
      { label: 'Score', value: m => `${m.home_score}-${m.away_score}${m.playoff ? ' (playoffs)' : ''}`, left: true },
      { label: 'Winner', value: m => winner(m.home_code, m.home_score, m.away_code, m.away_score), left: true, cls: () => 'good' }], rows); };
    matchups('greatest', r.greatest_matchups);
    const clankers = (id, rows) => { document.getElementById(id).innerHTML = table([
      { label: 'Period', value: 'period', left: true }, { label: 'Era', value: span, left: true }, { label: 'Teams', value: 'teams' },
      { label: 'Date', value: e => e.game.game_date, left: true }, { label: 'Home', value: e => e.game.home_code, left: true }, { label: 'Elo', value: e => num(e.game.home_elo_pre, 0) },
      { label: 'Away', value: e => e.game.away_code, left: true }, { label: 'Elo', value: e => num(e.game.away_elo_pre, 0) },
      { label: 'Combined', value: e => num(e.game.home_elo_pre + e.game.away_elo_pre, 0) },
      { label: 'Score', value: e => `${e.game.home_score}-${e.game.away_score}${e.game.playoff ? ' (playoffs)' : ''}`, left: true },
      { label: 'Winner', value: e => winner(e.game.home_code, e.game.home_score, e.game.away_code, e.game.away_score), left: true, cls: () => 'good' }], rows); };
    clankers('clankersNba', r.clankers_by_era.filter(e => e.league === 'nba')); clankers('clankersAba', r.clankers_by_era.filter(e => e.league === 'aba'));
    const champ = c => `${labelOf(c.season)} <span title="${esc(c.name)} · regular-season opponents ${num(c.regular_opponent_elo, 0)}, playoff opponents ${num(c.playoff_opponent_elo, 0)}, toughest ${esc(c.toughest_opponent_code)} (${num(c.toughest_opponent_elo, 0)})">${esc(c.code)}</span>`;
    document.getElementById('roadsByEra').innerHTML = table([
      { label: 'Period', value: 'period', left: true }, { label: 'Era', value: span, left: true }, { label: 'Teams', value: 'teams' }, { label: 'Titles', value: 'champions' },
      { label: 'Hardest road', value: e => champ(e.hardest), raw: true, left: true }, { label: 'Road', value: e => num(e.hardest.road, 0) }, { label: 'Toughest opp.', value: e => `${e.hardest.toughest_opponent_code} (${num(e.hardest.toughest_opponent_elo, 0)})`, left: true },
      { label: 'Easiest road', value: e => champ(e.easiest), raw: true, left: true }, { label: 'Road', value: e => num(e.easiest.road, 0) }], r.roads_by_era.filter(e => e.league === 'nba'));
    setStatus(status, '');
  } catch (e) { setStatus(status, e.message, true); }
})();
