// The Injury card page: one team's report lines priced with the arithmetic, and the two
// totals. The team and season come from the query string when a team code elsewhere opened
// the page.
(async () => {
  const { num, esc, table, getJSON, setStatus, labelOf, bonusText, whatIfSentence, query, homeBonusNote, playerSearch, whatIfControl } = NBA;
  const seasonSelect = document.getElementById('season');
  const cardStatus = document.getElementById('cardStatus');
  const search = playerSearch(document.getElementById('whoOptions'), document.getElementById('wiStatus'));
  const whatIfs = whatIfControl(search, () => { if (document.getElementById('cardTeam').value) loadCard(); });
  const homeQuery = () => { const v = document.getElementById('radv').value.trim(); return query(v === '' ? '' : `home_advantage=${encodeURIComponent(v)}`, whatIfs.param()); };
  const spanLabel = { return_date: 'return date', expected: 'expected run', measured: 'status curve', status: 'status default', season: 'season' };
  async function fillCardTeams(want) {
    const season = parseInt(seasonSelect.value);
    const select = document.getElementById('cardTeam');
    const was = want || select.value;
    try {
      const t = await getJSON(`/api/seasons/${season}/teams`);
      const teams = t.teams.slice().sort((a, b) => a.code.localeCompare(b.code));
      select.innerHTML = teams.map(x => `<option value="${esc(x.code)}">${esc(x.code)} · ${esc(x.name)}</option>`).join('');
      if (teams.some(x => x.code === was)) select.value = was;
      setStatus(cardStatus, '');
    } catch (e) { select.innerHTML = ''; setStatus(cardStatus, `no teams: ${e.message}`, true); }
  }
  // Only the latest request may fill the card.
  let cardRequest = 0;
  async function loadCard() {
    const season = parseInt(seasonSelect.value);
    const code = document.getElementById('cardTeam').value;
    if (!code) {
      ++cardRequest;
      document.getElementById('cardTotals').innerHTML = ''; document.getElementById('cardLines').innerHTML = '';
      setStatus(cardStatus, 'pick a team', true); return;
    }
    const mine = ++cardRequest;
    setStatus(cardStatus, 'loading…');
    // The address follows the card, so it can be bookmarked or shared.
    history.replaceState(null, '', `/season/injuries?season=${encodeURIComponent(season)}&team=${encodeURIComponent(code)}`);
    try {
      const c = await getJSON(`/api/seasons/${season}/teams/${encodeURIComponent(code)}/injury-card${homeQuery()}`);
      if (mine !== cardRequest) return;
      const priced = c.lines.filter(l => l.penalty_elo != null).length;
      document.getElementById('cardNote').textContent = c.note
        ? c.note
        : `${c.team_code} · ${c.team_name}, ${labelOf(c.season)}. Full-strength rating ${num(c.rating_elo, 0)} (${c.rating_is_preseason ? 'preseason, before any game' : `after the game of ${c.rating_date}`}; stored Elo ${num(c.rating_stored, 0)}${c.games_short ? `, with ${c.games_short} game${c.games_short === 1 ? '' : 's'} played short taken out` : ''}); today ${c.today} the team plays at ${num(c.rating_today, 0)}, ${num(c.penalty_today, 0)} below it. Report of ${c.report_date}: ${c.lines.length} line${c.lines.length === 1 ? '' : 's'}, ${priced} priced. ${c.listed.games} regular-season games remain, ${c.listed.games_covered} with a penalty as listed. ${bonusText(c.home_advantage, c.home_advantage_fitted)}.${whatIfSentence(c.what_ifs)}`;
      const totals = [
        { label: 'As listed', e: c.listed, note: 'each absence ends when the report, the curve or the status default says' },
        { label: 'Held to the last game', e: c.held, note: `today's absences (${num(c.penalty_today, 0)} Elo) kept through the last regular-season game, ${c.last_regular_game}` }];
      document.getElementById('cardTotals').innerHTML = c.note ? '' : table([
        { label: 'Total', value: 'label', left: true },
        { label: 'Games covered', value: t => `${t.e.games_covered} of ${t.e.games}`, title: 'Remaining regular-season games with a penalty' },
        { label: 'Elo × games', value: t => num(t.e.elo_games, 0), title: 'The penalties summed over the games covered' },
        { label: 'Mean penalty (Elo)', value: t => t.e.mean_penalty ? `−${num(t.e.mean_penalty, 1)}` : '0', cls: t => t.e.mean_penalty ? 'bad' : '', title: 'Elo × games divided by the remaining games: the figure the forecast table shows for a cutoff of today' },
        { label: 'Wins lost (first order)', value: t => num(t.e.wins_lost, 2), cls: t => t.e.wins_lost ? 'bad' : '', title: 'Win probability at the full rating less at the penalized one, summed over the games covered, at today\'s ratings with the opponents at full strength; the forecast simulates the season instead' },
        { label: 'How', value: 'note', left: true }], totals);
      document.getElementById('cardLines').innerHTML = c.lines.length ? table([
        { label: 'Player', value: 'name', left: true },
        { label: 'Status', value: 'status_text', left: true },
        { label: 'Return', value: l => l.return_date || '', left: true, title: 'The report\'s return date' },
        { label: 'Missed', value: l => l.missed_so_far == null ? '' : l.missed_so_far, title: 'Team games missed in a row up to the latest game with box scores' },
        { label: 'Worth (Elo)', value: l => l.worth_elo == null ? 'no estimate' : `${num(l.worth_elo, 0)}${l.se_elo ? ` ± ${num(l.se_elo, 0)}` : ''} (${l.worth_first_season && l.worth_first_season !== l.worth_season ? `${labelOf(l.worth_first_season)} to ${labelOf(l.worth_season)}` : labelOf(l.worth_season)}, ${{ box: 'box score', blend: 'missed games and box score', draft: 'draft prior', worth: 'missed games' }[l.worth_source] || l.worth_source})`, left: true, title: 'What the team loses per game without him, over the seasons shown: the with/without worth with the seasons pooled, the box-score worth with the seasons pooled by minutes and scaled by its measured share, the two combined by precision, or a rookie\'s draft prior' },
        { label: 'Weight', value: l => l.weight == null ? '' : `${num(100 * l.weight, 0)}% (${l.weight_source === 'measured' ? 'measured' : 'default'})`, left: true, title: 'The share of the worth taken off: the measured chance a line of this status misses the next game, or the status default' },
        { label: 'Penalty / game', value: l => l.penalty_elo == null ? '' : `${num(l.penalty_elo, 0)}${l.expected_penalty_elo != null && Math.abs(l.expected_penalty_elo - l.penalty_elo) >= 0.5 ? ` (${num(l.expected_penalty_elo, 0)} priced)` : ''}`, cls: l => l.penalty_elo ? 'bad' : '', title: 'Worth × weight, capped at 400. With an error on the worth, the games are priced at the mean of the draw held between 0 and 400, shown in brackets when it differs' },
        { label: 'Until', value: l => l.until ? `${l.until} (${spanLabel[l.span] || l.span})` : '', left: true, title: 'The last date the penalty applies, and what set it' },
        { label: 'Games covered', value: l => l.penalty_elo == null ? '' : l.games_covered, title: 'Remaining regular-season games up to that date' },
        { label: 'Elo × games', value: l => l.penalty_elo == null ? '' : num(l.elo_games, 0) },
        { label: 'Of the mean', value: l => l.penalty_elo == null ? '' : l.mean_penalty_elo ? `−${num(l.mean_penalty_elo, 1)}` : '0', title: 'This line\'s part of the team\'s mean penalty over the remaining games' },
        { label: 'Wins lost alone (first order)', value: l => l.penalty_elo == null ? '' : num(l.wins_lost_alone, 2), title: 'If this were the only absence, at today\'s ratings' }], c.lines) : '<p class="muted small">No line for this team.</p>';
      setStatus(cardStatus, '');
    } catch (e) { if (mine !== cardRequest) return; setStatus(cardStatus, e.message, true); document.getElementById('cardTotals').innerHTML = ''; document.getElementById('cardLines').innerHTML = ''; }
  }
  document.getElementById('cardLoad').addEventListener('click', loadCard);
  document.getElementById('cardTeam').addEventListener('change', loadCard);
  document.getElementById('radv').addEventListener('change', () => { if (document.getElementById('cardTeam').value) loadCard(); });
  seasonSelect.addEventListener('change', async () => { await fillCardTeams(); if (document.getElementById('cardTeam').value) loadCard(); });
  // The season and team asked for in the address, when a team code elsewhere opened the page.
  // The season is set before the bonus note and the team list read it, with no change event:
  // one fill and one load, so a second fill cannot land after the first and switch the team.
  const asked = new URLSearchParams(location.search);
  const season = asked.get('season');
  if (season && [...seasonSelect.options].some(o => o.value === season)) seasonSelect.value = season;
  homeBonusNote(seasonSelect, document.getElementById('radv'), document.getElementById('advNote'));
  await fillCardTeams(asked.get('team') || '');
  if (document.getElementById('cardTeam').value) loadCard();
})();
