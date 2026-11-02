(async () => {
  const { pct, num, esc, runJob, table, setStatus, chart, RESULT, getJSON, labelOf, bracketHtml, knockoutHtml } = NBA;
  // A probability cell shaded by its value; the text stays readable on every shade.
  const shade = p => p == null ? '' : `<span class="heat" style="background:rgba(37,99,235,${(0.08 + 0.72 * p).toFixed(2)});color:${p > 0.5 ? '#fff' : 'inherit'}">${pct(p, 0)}</span>`;
  const heatmap = (teams, pick, n, head) => {
    const cols = [{ label: 'Team', value: t => t.code, left: true }];
    for (let k = 0; k < n; k++) cols.push({ label: `${head}${k + 1}`, value: t => shade((pick(t) || [])[k]), raw: true });
    return table(cols, teams);
  };
  function renderCup(r) {
    const cup = r.cup;
    const note = document.getElementById('cupNote');
    if (!cup) {
      note.textContent = r.cup_note || (r.cup === undefined ? 'This forecast was run before the Cup forecast existed; run it again.' : '');
      document.getElementById('cupGroups').innerHTML = ''; document.getElementById('cupBracket').innerHTML = ''; document.getElementById('cupBracketNote').textContent = '';
      return;
    }
    const chance = {}; cup.teams.forEach(t => { chance[t.code] = t; });
    note.textContent = `${cup.group_games_played} of ${cup.group_games_total} group games played on or before the cutoff; the knockout formed in ${cup.iterations.toLocaleString()} simulated seasons.${cup.actual_winner ? ` The Cup was decided: ${cup.actual_winner} won it.` : ''} The group letters are the application's, not the league's.`;
    document.getElementById('cupGroups').innerHTML = cup.groups.map(g => `<div><h3 class="small">${esc(g.label)}</h3>${table([
      { label: 'Team', value: t => t.code, left: true }, { label: 'Group record', value: t => `${t.wins}-${t.losses}` }, { label: 'Diff', value: t => (t.diff >= 0 ? '+' : '') + t.diff },
      { label: 'Knockout', value: t => pct((chance[t.code] || {}).p_knockout || 0) }, { label: 'Final', value: t => pct((chance[t.code] || {}).p_final || 0) },
      { label: 'Wins Cup', value: t => pct((chance[t.code] || {}).p_winner || 0, 1), cls: () => 'good' }], g.teams)}</div>`).join('');
    const pb = cup.projected;
    if (!pb) { document.getElementById('cupBracket').innerHTML = '<p class="muted small">No simulated season formed a knockout.</p>'; document.getElementById('cupBracketNote').textContent = ''; return; }
    const times = c => c === 0 ? 'in none' : c === 1 ? 'in 1' : `in ${c.toLocaleString()}`;
    document.getElementById('cupBracketNote').textContent = `Each seed in turn to the team that held it most often among the teams not yet placed; the chances are exact at the cutoff date's ratings. This exact bracket occurred ${times(pb.exact_count)} of the ${pb.sampled.toLocaleString()} simulated seasons counted, these eight teams in any seeding ${times(pb.field_count)}, and the most frequent bracket of all ${times(pb.most_common_count)}.`;
    document.getElementById('cupBracket').innerHTML = knockoutHtml({ slots: pb.slots, rounds: 3, exact: pb.exact, actual: [], firstRound: pb.quarterfinals, champion: 'Cup winner', seriesWord: 'quarterfinal', finalWord: 'Final' });
  }
  function renderProjected(r) {
    const pb = r.projected_bracket;
    document.getElementById('projected').innerHTML = pb ? bracketHtml(pb.bracket, pb.exact, pb.first_round) : '<p class="muted small">No simulated season produced a full bracket.</p>';
    const times = c => c === 0 ? 'in none' : c === 1 ? 'in 1' : `in ${c.toLocaleString()}`;
    document.getElementById('bracketNote').textContent = pb ? `This exact seeding occurred ${times(pb.exact_count)} of the ${pb.sampled.toLocaleString()} simulated seasons counted, and these sixteen teams made the playoffs in any seeding ${times(pb.field_count)}. The most frequent seeding of all occurred ${times(pb.most_common_count)}.${pb.sampled < r.iterations ? ` The counts cover the first ${pb.sampled.toLocaleString()} of ${r.iterations.toLocaleString()} simulated seasons.` : ''}` : '';
    const east = r.teams.filter(t => t.conference === 'east'), west = r.teams.filter(t => t.conference === 'west');
    const byConf = (pick, n, head) => `<h3 class="small">East</h3>${heatmap(east, pick, n, head)}<h3 class="small">West</h3>${heatmap(west, pick, n, head)}`;
    document.getElementById('positions').innerHTML = byConf(t => t.position_distribution, Math.max(east.length, west.length), '');
    document.getElementById('seeds').innerHTML = byConf(t => t.seed_distribution, 8, 'S');
  }
  const status = document.getElementById('status');
  const seasons = await getJSON('/api/seasons').catch(() => []);
  const provenance = (season) => {
    const s = seasons.find(x => x.season === season);
    if (!s || !s.computed_game_count) return '';
    return ` Elo for this season is computed by the application (${s.computed_game_count.toLocaleString()} games, ${s.scheduled_game_count.toLocaleString()} not yet played), not imported from the FiveThirtyEight archive.`;
  };
  document.getElementById('preseason').addEventListener('change', e => document.getElementById('asOf').disabled = e.target.checked);
  document.getElementById('asOf').disabled = true;
  // The season in progress defaults to today: that is what "the forecast" means for it.
  // A past season keeps the preseason default, where the interesting question lives.
  // The local date, not the UTC one: a US evening is already tomorrow in UTC.
  const today = new Date(); const pad = n => String(n).padStart(2, '0');
  const todayIso = `${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`;
  const currentSeason = today.getMonth() >= 8 ? today.getFullYear() + 1 : today.getFullYear();
  let dateTouched = false;
  document.getElementById('preseason').addEventListener('change', () => { dateTouched = true; });
  const defaultDate = () => {
    if (dateTouched) return;
    const season = parseInt(document.getElementById('season').value);
    const pre = document.getElementById('preseason'); const asOf = document.getElementById('asOf');
    if (season === currentSeason) { pre.checked = false; asOf.disabled = false; if (!asOf.value) asOf.value = todayIso; }
    else { pre.checked = true; asOf.disabled = true; }
  };
  document.getElementById('season').addEventListener('change', defaultDate);
  defaultDate();
  // The regular-season home-court bonus is fitted per season; the field shows the fit as its
  // placeholder and a typed value replaces it. Playoff games keep the archive's 100.
  let advRequest = 0;
  const showAdvantage = async () => {
    const season = parseInt(document.getElementById('season').value);
    const radv = document.getElementById('radv'); const note = document.getElementById('advNote');
    const mine = ++advRequest;
    const h = await getJSON(`/api/seasons/${season}/home-advantage`).catch(() => null);
    if (mine !== advRequest) return;
    if (!h) { radv.placeholder = '100'; note.textContent = ''; return; }
    radv.placeholder = num(h.regular, 0);
    note.textContent = h.fitted_from
      ? `Regular-season home-court bonus for ${labelOf(season)}: ${num(h.regular, 0)} Elo, fitted on ${h.games.toLocaleString()} regular-season games of ${labelOf(h.fitted_from[0])} to ${labelOf(h.fitted_from[1])}. Playoff games use ${num(h.playoff, 0)}.`
      : `Regular-season home-court bonus for ${labelOf(season)}: ${num(h.regular, 0)} Elo (the default; fewer than 2,000 earlier regular-season games are stored). Playoff games use ${num(h.playoff, 0)}.`;
  };
  document.getElementById('season').addEventListener('change', showAdvantage);
  // The injury card: one team's report lines with the arithmetic, and the two totals.
  const cardStatus = document.getElementById('cardStatus');
  async function fillCardTeams() {
    const season = parseInt(document.getElementById('season').value);
    const select = document.getElementById('cardTeam');
    const was = select.value;
    try {
      const t = await getJSON(`/api/seasons/${season}/teams`);
      const teams = t.teams.slice().sort((a, b) => a.code.localeCompare(b.code));
      select.innerHTML = teams.map(x => `<option value="${esc(x.code)}">${esc(x.code)} · ${esc(x.name)}</option>`).join('');
      if (teams.some(x => x.code === was)) select.value = was;
      setStatus(cardStatus, '');
    } catch (e) { select.innerHTML = ''; setStatus(cardStatus, `no teams: ${e.message}`, true); }
  }
  const spanLabel = { return_date: 'return date', expected: 'expected run', measured: 'status curve', status: 'status default', season: 'season' };
  // A card once asked for is reloaded when the home bonus changes, even after a failed load;
  // only the latest request may fill it.
  let cardAsked = false, cardRequest = 0;
  async function loadCard() {
    const season = parseInt(document.getElementById('season').value);
    const code = document.getElementById('cardTeam').value;
    if (!code) {
      // No team: any request in flight is dropped and the old card cleared.
      ++cardRequest;
      document.getElementById('cardTotals').innerHTML = ''; document.getElementById('cardLines').innerHTML = '';
      setStatus(cardStatus, 'pick a team', true); return;
    }
    cardAsked = true;
    const mine = ++cardRequest;
    setStatus(cardStatus, 'loading…');
    try {
      const c = await getJSON(`/api/seasons/${season}/teams/${encodeURIComponent(code)}/injury-card${homeQuery()}`);
      if (mine !== cardRequest) return;
      const priced = c.lines.filter(l => l.penalty_elo != null).length;
      document.getElementById('cardNote').textContent = c.note
        ? c.note
        : `${c.team_code} · ${c.team_name}, ${labelOf(c.season)}. Full-strength rating ${num(c.rating_elo, 0)} (${c.rating_is_preseason ? 'preseason, before any game' : `after the game of ${c.rating_date}`}; stored Elo ${num(c.rating_stored, 0)}${c.games_short ? `, with ${c.games_short} game${c.games_short === 1 ? '' : 's'} played short taken out` : ''}); today ${c.today} the team plays at ${num(c.rating_today, 0)}, ${num(c.penalty_today, 0)} below it. Report of ${c.report_date}: ${c.lines.length} line${c.lines.length === 1 ? '' : 's'}, ${priced} priced. ${c.listed.games} regular-season games remain, ${c.listed.games_covered} with a penalty as listed. ${bonusText(c.home_advantage, c.home_advantage_fitted)}.`;
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
  document.getElementById('season').addEventListener('change', fillCardTeams);
  // Tonight: the next night's games at the ratings the teams play at, and every team's rating tonight.
  // One team's spread as a sportsbook writes it (the points it gets, + or −, from its expected
  // margin), and an American line with its sign.
  const teamSpread = m => Math.abs(m) < 0.25 ? 'pick' : `${m > 0 ? '−' : '+'}${num(Math.abs(m), 1)}`;
  const line = o => (o > 0 ? '+' : '−') + Math.round(Math.abs(o));
  // The Reg-season home adv. field, when set, prices the Tonight panel and the injury card too.
  const homeQuery = () => { const v = document.getElementById('radv').value.trim(); return v === '' ? '' : `?home_advantage=${encodeURIComponent(v)}`; };
  // The bar a bet's expected return must clear to be flagged, as the field gives it in percent.
  const minReturn = () => { const field = document.getElementById('minret'); const v = parseFloat(field.value); if (Number.isFinite(v) && v >= 0 && v <= 100) return v / 100; field.value = '5'; return 0.05; };
  const minReturnQuery = () => `?min_return=${encodeURIComponent(minReturn())}`;
  const bonusText = (h, fitted) => Math.abs(h - fitted) < 0.05 ? `Home bonus ${num(h, 0)} (fitted)` : `Home bonus ${num(h, 0)} (yours; fitted ${num(fitted, num(h, 0) === num(fitted, 0) ? 1 : 0)})`;
  const teamLink = (code, season) => `<a href="#injuryCard" class="card-link" data-team="${esc(code)}" data-season="${esc(String(season))}" title="Open the team's injury card">${esc(code)}</a>`;
  const statusNames = { out: 'out', out_for_season: 'out for season', absent: 'absent', doubtful: 'doubtful', questionable: 'questionable', day_to_day: 'day-to-day', probable: 'probable', available: 'available', unknown: 'listed' };
  let tonightRequest = 0;
  let marketSeen = { key: null, at: 0, market: null };
  async function loadTonight() {
    const season = parseInt(document.getElementById('season').value);
    const note = document.getElementById('tonightNote');
    const mine = ++tonightRequest;
    try {
      const t = await getJSON(`/api/seasons/${season}/tonight${homeQuery()}`);
      if (mine !== tonightRequest) return;
      // The night may not be today: an off day, a break, or every game of today played.
      const when = t.night === t.as_of ? 'tonight' : `on ${t.night}`;
      note.textContent = t.note ? t.note : t.night
        ? `${labelOf(t.season)}: ${t.games.length} game${t.games.length === 1 ? '' : 's'} ${t.games_played ? `still to play of ${t.games.length + t.games_played} ` : ''}on ${t.night}${t.night === t.as_of ? ' (today)' : ', the next night with a game'}, read as of ${t.as_of} with the report of ${t.report_date}. ${bonusText(t.home_advantage, t.home_advantage_fitted)}. Ratings are at full strength: ${t.games_played_short ? `${t.games_played_short} team game${t.games_played_short === 1 ? '' : 's'} played short this season are taken out of them` : 'no game played short is on record, so they equal the stored Elo'} (${t.rating_model}).`
        : `${labelOf(t.season)}: no regular-season game remains on or after ${t.as_of}.`;
      // Laid out as a sportsbook lists a game: the visitor on top, the home team under it with
      // an @, each with its own spread and moneyline beside the sportsbook's; then each side's
      // win chance at full strength, on the night and in the market. `market` is undefined
      // while the lines load, and a game without a line shows dashes.
      const renderGames = market => {
        const byGame = new Map(((market && market.games) || []).map(m => [m.game_id, m]));
        // The sportsbook's name when every line is from the same one.
        // Only the lines of the night's games count, should the market hold others.
        const nightIds = new Set(t.games.map(g => g.game_id));
        const lines = ((market && market.games) || []).filter(m => nightIds.has(m.game_id));
        const providers = [...new Set(lines.map(m => m.provider))];
        const book = providers.length === 1 ? providers[0] : 'Sportsbook';
        const sideRow = (g, home) => {
          const code = home ? g.home_code : g.away_code, rating = home ? g.home_rating : g.away_rating;
          const rated = home ? g.home_rated : g.away_rated, penalty = home ? g.home_penalty : g.away_penalty;
          const playsAt = home ? g.home_tonight : g.away_tonight;
          const margin = home ? g.home_margin_tonight : -g.home_margin_tonight;
          // The home chance is rounded and the visitor's is the rest of 100, so a game's two add up.
          const share = p => { if (p == null || !Number.isFinite(p)) return NaN; const h = Math.round(100 * p); return home ? h : 100 - h; };
          const shown = x => Number.isFinite(x) ? `${x}%` : '–';
          const moved = (a, b) => !Number.isFinite(a) || !Number.isFinite(b) || a === b ? '' : b < a ? 'bad' : 'good';
          const full = share(g.p_home_full), night = share(g.p_home_tonight);
          const m = byGame.get(g.game_id);
          const wait = market === undefined ? '…' : '–';
          // The sportsbook's spread is the home team's, negative when it gives points.
          const spreadOf = (sp, odds) => sp == null ? wait : `${sp === 0 ? 'pick' : `${sp > 0 ? '+' : '−'}${num(Math.abs(sp), 1)}`}${odds == null ? '' : ` (${line(odds)})`}`;
          const bookSpread = m ? spreadOf(m.home_spread == null ? null : (home ? m.home_spread : -m.home_spread), home ? m.home_spread_odds : m.away_spread_odds) : wait;
          const bookLine = m && (home ? m.home_moneyline : m.away_moneyline) != null ? line(home ? m.home_moneyline : m.away_moneyline) : wait;
          // The sportsbook's cell a flagged bet is on is marked.
          const flagged = kind => m && (m.edges || []).some(e => e.side === (home ? 'home' : 'away') && e.kind === kind);
          const pickClass = kind => flagged(kind) ? ' pick' : '';
          const marketWins = m && m.p_home != null ? share(m.p_home) : NaN;
          const gap = Number.isFinite(night) && Number.isFinite(marketWins) ? night - marketWins : NaN;
          const team = `${home ? '@ ' : ''}${teamLink(code, t.season)} ${num(rating, 0)}${rated ? '' : '*'}${penalty ? ` → <span class="bad">${num(playsAt, 0)}</span>` : ''}${home && g.neutral ? ' (neutral)' : ''}`;
          return `<tr><td class="l">${team}</td><td>${esc(teamSpread(margin))}</td><td class="book${pickClass('spread')}">${flagged('spread') ? '★ ' : ''}${esc(bookSpread)}</td><td>${esc(line(home ? g.home_moneyline_tonight : g.away_moneyline_tonight))}</td><td class="book${pickClass('moneyline')}">${flagged('moneyline') ? '★ ' : ''}${esc(bookLine)}</td><td>${shown(full)}</td><td class="${moved(full, night)}">${shown(night)}</td><td class="book">${Number.isFinite(marketWins) ? `${marketWins}%` : esc(wait)}</td><td>${Number.isFinite(gap) ? `${gap > 0 ? '+' : gap < 0 ? '−' : ''}${Math.abs(gap)}` : esc(wait)}</td></tr>`;
        };
        // Each column names whose number it is: the model's, or the sportsbook's by its name;
        // the sportsbook's columns are shaded, head and body.
        const heads = [
          ['Team', `The visitor, then the home team after @: its rating and, when it is missing players, the rating it plays at ${when}; * marks a team without a rating, read at the league mean`],
          ['Model spread', `The model's spread ${when}: the points the team gives (−) or gets (+), from the rating gap with the home bonus at 28 Elo a point, as a sportsbook writes it`],
          [`${book} spread`, `${book}'s spread for the team, with the price of the bet in brackets`, true],
          ['Model moneyline', `The model's fair American odds ${when}, from its win chance with no bookmaker margin: the favourite risks the amount shown to win 100, 100 on the underdog wins the amount shown`],
          [`${book} moneyline`, `${book}'s moneyline for the team, its margin included: the favourite must risk more to win 100, and 100 on the underdog wins less, than at a fair line`, true],
          ['Model wins, full strength', 'The model: the chance the team wins with both teams at full strength'],
          [`Model wins ${when}`, `The model: the chance the team wins at the ratings the teams play at ${when}; red when it fell from full strength, green when it rose`],
          [`${book} wins`, `The chance ${book}'s two moneylines imply for the team once its margin is taken out: each implied chance divided by their sum`, true],
          [`Model − ${book}`, `The model's chance ${when} less ${book}'s, in percentage points: positive when the model rates the team higher than ${book} does`]];
        document.getElementById('tonightGames').innerHTML = t.games.length
          ? `<table class="games"><thead><tr>${heads.map(([h, title, isBook], i) => `<th class="${i ? '' : 'l'}${isBook ? ' book' : ''}" title="${esc(title)}">${esc(h)}</th>`).join('')}</tr></thead>${t.games.map(g => `<tbody>${sideRow(g, false)}${sideRow(g, true)}</tbody>`).join('')}</table>`
          : '';
        // The same explanations as the headers' tooltips, laid out for a reader who does not hover.
        document.getElementById('tonightLegend').innerHTML = heads.map(([h, title, isBook]) => `<dt class="${isBook ? 'book' : ''}">${esc(h)}</dt><dd>${esc(title)}</dd>`).join('');
        document.getElementById('tonightLegendBox').hidden = !t.games.length;
        const marketNote = document.getElementById('tonightMarketNote');
        renderPicks(market, t, book, lines);
        if (market === undefined) marketNote.textContent = t.games.length ? 'Reading the sportsbook lines…' : '';
        else if (market.note) marketNote.textContent = `Market: ${market.note}.`;
        else {
          const margins = lines.map(m => m.book_margin).filter(x => x != null);
          const mean = margins.length ? margins.reduce((a, b) => a + b, 0) / margins.length : null;
          const sourceNames = { 'the-odds-api': 'The Odds API', espn: "ESPN's scoreboard" };
          const quota = market.quota && market.quota.remaining != null ? ` The Odds API has ${market.quota.remaining.toLocaleString()} credit${market.quota.remaining === 1 ? '' : 's'} left${market.quota.last_cost ? ` at ${market.quota.last_cost} a read` : ''}.` : '';
          marketNote.textContent = `Market: ${providers.join(', ')} lines from ${sourceNames[market.source] || market.source || 'the market source'}, read ${new Date(market.read_at).toLocaleTimeString()}, ${lines.length} of ${t.games.length} game${t.games.length === 1 ? '' : 's'}${mean == null ? '' : `; the bookmaker's margin averages ${num(100 * mean, 1)}% and is taken out of ${book} wins`}.${market.source_note ? ` ${market.source_note}.` : ''} Lines move until tip-off; a line read after the start is not saved.${quota}`;
        }
      };
      // The bets the edge rule flags, each with its arithmetic, best first.
      const renderPicks = (market, t, book, lines) => {
        const box = document.getElementById('tonightPicks');
        if (!market || market.note || !t.games.length) { box.innerHTML = ''; return; }
        const byId = new Map(t.games.map(g => [g.game_id, g]));
        const signed = (x, d) => `${x > 0 ? '+' : x < 0 ? '−' : ''}${num(Math.abs(x), d)}`;
        const picks = [];
        for (const m of lines) {
          const g = byId.get(m.game_id); if (!g) continue;
          for (const e of (m.edges || [])) {
            const code = e.side === 'home' ? g.home_code : g.away_code, other = e.side === 'home' ? g.away_code : g.home_code;
            const margin = e.side === 'home' ? g.home_margin_tonight : -g.home_margin_tonight;
            const needs = `the price needs ${pct(e.p_price, 0)}`;
            const ret = `a unit returns ${signed(e.expected_return, 2)} on average if the model is right`;
            const text = e.kind === 'moneyline'
              ? `${code} ${line(e.price)} to beat ${other} at ${book}: the model gives ${code} ${pct(e.p_model, 0)} to win, ${needs}; ${ret}.`
              : `${code} ${e.line === 0 ? 'pick' : signed(e.line, 1)} (${line(e.price)}) against ${other} at ${book}: the model's margin is ${code} by ${num(margin, 1)}, so ${code} covers ${pct(e.p_model, 0)} of the time${e.p_push > 0.005 ? ` and pushes ${pct(e.p_push, 0)}` : ''}, ${needs}; ${ret}.`;
            picks.push({ r: e.expected_return, text });
          }
        }
        picks.sort((a, b) => b.r - a.r);
        const sc = market.scatter || {};
        const basis = sc.games ? `Margins scatter ${num(sc.sigma, 1)} points around the model's expected margin, measured over ${sc.games.toLocaleString()} games (${labelOf(sc.from)} to ${labelOf(sc.to)}).` : `Margins are taken to scatter ${num(sc.sigma, 1)} points around the model's expected margin (too few played games to measure).`;
        const bar = `${num(100 * market.min_return, 0)}%`;
        box.innerHTML = picks.length
          ? `<p class="muted" style="margin:.5rem 0 0">Bets the model likes at a bar of ${esc(bar)} of the stake (its chance of winning times the payout at ${esc(book)}'s price, less its chance of losing; the price carries the margin, so the bar is above it), best first:</p><ul>${picks.map(p => `<li>${esc(p.text)}</li>`).join('')}</ul><p class="muted" style="margin:.3rem 0 0">The edge is the model's disagreement with ${esc(book)}, which is unproven until the Model vs market panel's flagged record says otherwise. ${esc(basis)}</p>`
          : `<p class="muted" style="margin:.5rem 0 0">No bet reaches the bar of ${esc(bar)} of the stake at ${esc(book)}'s prices. ${esc(basis)}</p>`;
      };
      // The lines do not depend on the home bonus: a market read for this season, night and
      // bar in the last minute is reused rather than asked for again (the server keeps the
      // lines longer).
      const key = `${season}|${t.night}|${minReturn()}`;
      if (marketSeen.key === key && Date.now() - marketSeen.at < 60 * 1000) renderGames(marketSeen.market);
      else {
        renderGames(undefined);
        if (t.games.length) {
          getJSON(`/api/seasons/${season}/tonight/market${minReturnQuery()}`)
            .then(m => { if (!m.note) marketSeen = { key, at: Date.now(), market: m }; if (mine === tonightRequest) renderGames(m); })
            .catch(e => { if (mine === tonightRequest) renderGames({ games: [], note: `the lines could not be read (${e.message})` }); });
        }
      }
      document.getElementById('tonightTeams').innerHTML = t.teams.length ? table([
        { label: 'Team', value: x => teamLink(x.code, t.season), raw: true, left: true },
        { label: 'Elo', value: x => num(x.stored_rating, 0), title: 'The stored Elo as of the date: the absences of the games played are in it, since a loss played short lowered it like any other' },
        { label: 'Full strength', value: x => num(x.rating, 0), cls: x => Math.abs(x.rating - x.stored_rating) >= 0.5 ? (x.rating > x.stored_rating ? 'good' : 'bad') : '', title: 'The season\'s games replayed with the Elo each side was missing in each game (listed on that day\'s report and absent from the box score) taken out of the expectation: the rating of the full team, which tonight\'s absences come off' },
        { label: 'Games short', value: x => x.games_short || '', title: 'Games the team played with players missing, whose absences the full-strength rating takes out' },
        { label: `Out ${when} (Elo)`, value: x => x.penalty ? `−${num(x.penalty, 0)}` : '', cls: x => x.penalty ? 'bad' : '', title: 'The penalties of the absences that cover the night, each the mean of its draw when the worth has an error' },
        { label: `Elo ${when}`, value: x => num(x.rating_tonight, 0), cls: x => x.penalty ? 'bad' : '', title: `The rating the team plays at ${when}` },
        { label: 'Plays', value: x => x.opponent ? `${x.home ? 'vs' : 'at'} ${x.opponent}` : '', left: true, title: 'Blank for a team not playing, or whose game is already played' },
        { label: 'Who is out', value: x => x.out.map(o => `${o.name} (${num(o.penalty_elo, 0)}, ${statusNames[o.status] || o.status_text.toLowerCase()} to ${o.until})`).join('; '), left: true, title: 'Each absence covering the night: the penalty it carries, the status and the last date it applies' }], t.teams) : '';
    } catch (e) { note.textContent = `Tonight unavailable: ${e.message}`; document.getElementById('tonightGames').innerHTML = ''; document.getElementById('tonightTeams').innerHTML = ''; document.getElementById('tonightMarketNote').textContent = ''; document.getElementById('tonightPicks').innerHTML = ''; document.getElementById('tonightLegendBox').hidden = true; }
  }
  document.getElementById('season').addEventListener('change', loadTonight);
  document.getElementById('minret').addEventListener('change', () => { loadTonight(); loadTrack(); });
  // Model vs market: the stored snapshots of the season scored against the results.
  let trackRequest = 0;
  async function loadTrack() {
    const season = parseInt(document.getElementById('season').value);
    const note = document.getElementById('trackNote');
    const mine = ++trackRequest;
    try {
      const r = await getJSON(`/api/seasons/${season}/market/track${minReturnQuery()}`);
      if (mine !== trackRequest) return;
      note.textContent = r.note ? r.note : `${labelOf(r.season)}: ${r.snapshots} game${r.snapshots === 1 ? '' : 's'} saved, ${r.scored} scored, ${r.pending} waiting for a result${r.unmatched ? `, ${r.unmatched} matching no stored game (a game moved to another date)` : ''}. The model saving now is ${r.current_model}.`;
      const signedUnits = u => `${u > 0 ? '+' : u < 0 ? '−' : ''}${num(Math.abs(u), 2)}`;
      const record = (w, l, p) => `${w}-${l}${p ? `-${p}` : ''}`;
      const rows = r.scored ? [{ key: 'All games', ...r.overall }]
        .concat(r.by_model.map(g => ({ ...g, key: `Model ${g.key}${g.key === r.current_model ? ' (now)' : ''}` })))
        .concat(r.by_month.map(g => ({ ...g, key: `Month ${g.key}` }))) : [];
      document.getElementById('trackSummary').innerHTML = rows.length ? table([
        { label: 'Group', value: g => g.key, left: true },
        { label: 'Games', value: g => g.games },
        { label: 'Brier, model', value: g => g.priced ? num(g.brier_model, 3) : '', cls: g => g.priced && g.brier_model < g.brier_market ? 'good' : '', title: 'Mean squared error of the home chance against the result, over the games the sportsbook priced; lower is better' },
        { label: 'Brier, market', value: g => g.priced ? num(g.brier_market, 3) : '' },
        { label: 'Log loss, model', value: g => g.priced ? num(g.log_loss_model, 3) : '', cls: g => g.priced && g.log_loss_model < g.log_loss_market ? 'good' : '', title: 'Mean of minus the log of the chance the forecast gave to what happened; lower is better' },
        { label: 'Log loss, market', value: g => g.priced ? num(g.log_loss_market, 3) : '' },
        { label: 'Lean W-L', value: g => record(g.lean_won, g.lean_lost), title: 'The side whose chance the model put above the market\'s, bet on the moneyline' },
        { label: 'Lean units', value: g => signedUnits(g.lean_units), cls: g => g.lean_units > 0 ? 'good' : g.lean_units < 0 ? 'bad' : '', title: 'One unit a game at the sportsbook\'s moneyline' },
        { label: 'Per bet', value: g => (g.lean_won + g.lean_lost) ? signedUnits(g.lean_return) : '' },
        { label: 'Spread W-L-P', value: g => record(g.spread_won, g.spread_lost, g.spread_push), title: 'The side the model\'s margin took against the sportsbook\'s spread' },
        { label: 'Spread units', value: g => signedUnits(g.spread_units), cls: g => g.spread_units > 0 ? 'good' : g.spread_units < 0 ? 'bad' : '', title: 'One unit a game at the spread\'s price' },
        { label: 'Per bet', value: g => (g.spread_won + g.spread_lost + g.spread_push) ? signedUnits(g.spread_return) : '' },
        { label: 'Flagged W-L-P', value: g => record(g.flagged_won, g.flagged_lost, g.flagged_push), title: `The bets the edge rule flagged on the stored line at the bar of ${num(100 * r.min_return, 0)}% of the stake: the ones the Tonight panel would have marked` },
        { label: 'Flagged units', value: g => signedUnits(g.flagged_units), cls: g => g.flagged_units > 0 ? 'good' : g.flagged_units < 0 ? 'bad' : '', title: 'One unit a flagged bet at its price' },
        { label: 'Per bet', value: g => (g.flagged_won + g.flagged_lost + g.flagged_push) ? signedUnits(g.flagged_return) : '' }], rows) : '';
      const sideCode = (g, side) => side === 'home' ? g.snapshot.home_code : g.snapshot.away_code;
      const bet = (side, result, g) => side ? `${sideCode(g, side)} ${{ won: 'won', lost: 'lost', push: 'push' }[result] || ''}` : '';
      document.getElementById('trackGames').innerHTML = r.games.length ? table([
        { label: 'Date', value: g => g.snapshot.game_date, left: true },
        { label: 'Game', value: g => `${g.snapshot.away_code} ${g.away_score} @ ${g.snapshot.home_code} ${g.home_score}`, left: true },
        { label: 'Model, home wins', value: g => pct(g.snapshot.p_home_model, 0), title: 'The model\'s home chance at the ratings the teams played at' },
        { label: 'Market, home wins', value: g => g.snapshot.p_home_market == null ? '' : pct(g.snapshot.p_home_market, 0), title: 'The sportsbook\'s home chance with its margin taken out' },
        { label: 'Spread', value: g => g.snapshot.home_spread == null ? '' : `${g.snapshot.home_code} ${g.snapshot.home_spread > 0 ? '+' : g.snapshot.home_spread < 0 ? '−' : ''}${num(Math.abs(g.snapshot.home_spread), 1)}`, title: 'The sportsbook\'s spread for the home team in the last save before the start' },
        { label: 'Lean', value: g => bet(g.lean, g.lean_result, g), left: true },
        { label: 'Units', value: g => g.lean_units == null ? '' : signedUnits(g.lean_units), cls: g => g.lean_units > 0 ? 'good' : g.lean_units < 0 ? 'bad' : '' },
        { label: 'Spread bet', value: g => bet(g.spread_side, g.spread_result, g), left: true },
        { label: 'Units', value: g => g.spread_units == null ? '' : signedUnits(g.spread_units), cls: g => g.spread_units > 0 ? 'good' : g.spread_units < 0 ? 'bad' : '' },
        { label: 'Flagged', value: g => g.flagged.map(f => `${sideCode(g, f.edge.side)} ${f.edge.kind === 'spread' ? `${f.edge.line > 0 ? '+' : f.edge.line < 0 ? '−' : ''}${num(Math.abs(f.edge.line), 1)} ` : ''}${line(f.edge.price)} ${f.result} (${signedUnits(f.units)})`).join('; '), left: true, title: 'The bets the edge rule flagged on the stored line, each at its price' },
        { label: 'Model', value: g => g.snapshot.model_version, left: true }], r.games) : '';
    } catch (e) { if (mine === trackRequest) { note.textContent = `Model vs market unavailable: ${e.message}`; document.getElementById('trackSummary').innerHTML = ''; document.getElementById('trackGames').innerHTML = ''; } }
  }
  document.getElementById('season').addEventListener('change', loadTrack);
  loadTrack();
  // A home bonus typed in the field prices the night and an open injury card again.
  document.getElementById('radv').addEventListener('change', () => {
    loadTonight();
    if (cardAsked && document.getElementById('cardTeam').value) loadCard();
  });
  loadTonight();
  document.addEventListener('click', async e => {
    const a = e.target.closest('a.card-link'); if (!a) return;
    e.preventDefault();
    // The card follows the forecast's season, which the select may have left.
    const seasonSelect = document.getElementById('season');
    if (seasonSelect.value !== a.dataset.season) { seasonSelect.value = a.dataset.season; seasonSelect.dispatchEvent(new Event('change')); }
    await fillCardTeams();
    document.getElementById('cardTeam').value = a.dataset.team;
    loadCard();
    document.getElementById('injuryCard').scrollIntoView({ behavior: 'smooth' });
  });
  fillCardTeams();
  showAdvantage();

  // Players out: absences the user adds, priced with the player's worth.
  const absences = [];
  let found = new Map();
  const whoStatus = document.getElementById('whoStatus');
  const seasonSpan = () => { const s = parseInt(document.getElementById('season').value); return [`${s - 1}-10-01`, `${s}-06-30`]; };
  function renderAbsences() {
    document.getElementById('absences').innerHTML = absences.map((a, i) =>
      `<li>${esc(a.name)} (${esc(a.team_code)}) out ${esc(a.from)} to ${esc(a.to)}: −${num(a.elo, 0)} Elo${a.se_elo ? ` ± ${num(a.se_elo, 0)}` : ''} from the ${esc(a.source)} <button type="button" data-remove="${i}" class="small">remove</button></li>`).join('');
  }
  document.getElementById('absences').addEventListener('click', e => {
    const b = e.target.closest('button[data-remove]'); if (!b) return;
    absences.splice(Number(b.dataset.remove), 1); renderAbsences();
  });
  let searchTimer = null;
  document.getElementById('whoName').addEventListener('input', e => {
    const q = e.target.value.trim();
    clearTimeout(searchTimer);
    if (q.length < 2) return;
    searchTimer = setTimeout(async () => {
      try {
        const r = await getJSON(`/api/players?q=${encodeURIComponent(q)}`);
        // Two players of one name are told apart by team or identifier in the label.
        const names = new Map(); r.players.forEach(p => { const n = `${p.first_name} ${p.last_name}`.trim(); names.set(n, (names.get(n) || 0) + 1); });
        found = new Map(r.players.map(p => { const n = `${p.first_name} ${p.last_name}`.trim(); return [names.get(n) > 1 ? `${n} (${p.team_code || `#${p.id}`})` : n, p]; }));
        document.getElementById('whoOptions').innerHTML = [...found.keys()].map(n => `<option value="${esc(n)}"></option>`).join('');
      } catch (err) { setStatus(whoStatus, err.message, true); }
    }, 250);
  });
  document.getElementById('whoAdd').addEventListener('click', async () => {
    const name = document.getElementById('whoName').value.trim();
    const player = found.get(name);
    if (!player) { setStatus(whoStatus, 'Pick a player from the list first.', true); return; }
    const season = parseInt(document.getElementById('season').value);
    const [start, end] = seasonSpan();
    const asOf = document.getElementById('preseason').checked ? '' : document.getElementById('asOf').value;
    const from = document.getElementById('whoFrom').value || asOf || start;
    const to = document.getElementById('whoTo').value || end;
    if (to < from) { setStatus(whoStatus, 'The span ends before it starts.', true); return; }
    const want = document.getElementById('whoEstimate').value;
    setStatus(whoStatus, 'pricing…');
    try {
      const w = await getJSON(`/api/players/${player.id}/worth`);
      const worth = w.seasons.find(s => s.season === season) || null;
      const box = (w.box_seasons || []).filter(b => b.season === season).map(b => b.row).sort((a, b) => b.minutes - a.minutes)[0] || null;
      let pick = null;
      if (want === 'worth' || (want === 'auto' && worth)) pick = worth ? { elo: worth.worth_elo, se_elo: worth.se_elo, team_id: worth.teams[worth.teams.length - 1], source: 'missed games' } : null;
      else if (box) pick = { elo: box.worth_elo, se_elo: 0, team_id: box.team_id, source: 'box score' };
      if (!pick) { setStatus(whoStatus, `No ${want === 'box' ? 'box-score' : want === 'worth' ? 'with/without' : ''} estimate for ${name} in ${labelOf(season)}.`, true); return; }
      if (pick.elo <= 0) { setStatus(whoStatus, `${name}'s estimate is ${num(pick.elo, 0)} Elo: an absence that costs nothing is not added.`, true); return; }
      const teamCode = (w.teams || {})[String(pick.team_id)] || player.team_code || `team ${pick.team_id}`;
      const entry = { name, player_id: player.id, team_id: pick.team_id, team_code: teamCode, from, to, elo: Math.min(pick.elo, 400), se_elo: Math.min(pick.se_elo, 400), source: pick.source };
      // One span per player: a second Add replaces the first entry.
      const existing = absences.findIndex(a => a.player_id === player.id);
      if (existing >= 0) { absences[existing] = entry; setStatus(whoStatus, `${name} replaced.`); } else { absences.push(entry); setStatus(whoStatus, ''); }
      renderAbsences();
    } catch (err) { setStatus(whoStatus, err.message, true); }
  });
  document.getElementById('run').addEventListener('click', async () => {
    const btn = document.getElementById('run'); btn.disabled = true;
    const pk = document.getElementById('pk').value; const radv = document.getElementById('radv').value;
    const p = { season: parseInt(document.getElementById('season').value), iterations: parseInt(document.getElementById('iterations').value), seed: parseInt(document.getElementById('seed').value) || 0,
      home_advantage: parseFloat(document.getElementById('adv').value), regular_home_advantage: radv === '' ? null : parseFloat(radv), regular_season_k: parseFloat(document.getElementById('k').value), playoff_k: pk === '' ? null : parseFloat(pk), playoff_scale: 1.0 };
    if (!document.getElementById('preseason').checked && document.getElementById('asOf').value) p.as_of = document.getElementById('asOf').value;
    if (document.getElementById('injuries').checked) p.use_injury_report = true;
    if (absences.length) p.absences = absences.map(a => ({ team_id: a.team_id, from: a.from, to: a.to, elo: a.elo, se_elo: a.se_elo, player_id: a.player_id }));
    if (document.getElementById('sampleWorth').checked) p.sample_worth = true;
    if (document.getElementById('holdAbsences').checked) p.hold_absences = true;
    const name = document.getElementById('name').value.trim(); if (name) p.name = name;
    try {
      const job = await runJob('/api/jobs/season', p, j => setStatus(status, `${j.status} ${pct(j.progress, 0)}`));
      const r = job.result;
      document.getElementById('note').textContent = `${r.information_cutoff_note} Games used: ${r.games_played}; regular-season games simulated: ${r.games_remaining}; qualification format: ${r.qualification}. ${r.iterations.toLocaleString()} simulated seasons in ${r.elapsed_ms} ms.${provenance(p.season)} ${r.absences_applied ? ` ${r.absences_applied} absence${r.absences_applied === 1 ? '' : 's'} priced in${r.absences_sampled ? `, ${r.absences_sampled} with the worth drawn from its uncertainty in every simulated season` : ''}; see the Players page.` : p.use_injury_report ? ' The injury report priced nothing: no report is stored for this season, or no listed player has a worth; see the Players page.' : ''}${r.held ? ` Held: the season run again with the ${r.held.absences_held} absence${r.held.absences_held === 1 ? '' : 's'} covering ${r.as_of} kept to ${r.held.held_to}; the "held" columns are the forecast if the roster of ${r.as_of} were the roster all season, the bound the dated forecast sits above.` : p.hold_absences ? ' No absence covering the cutoff date ends before the last regular-season game, so there is no held run.' : ''}`;
      const held = new Map((r.held ? r.held.teams : []).map(h => [h.team_id, h]));
      const heldCol = (label, value, title) => ({ label, value: t => { const h = held.get(t.team_id); return h ? value(h) : ''; }, cls: () => 'muted', title });
      const cols = [
        { label: 'Team', value: t => `<a href="#injuryCard" class="card-link" data-team="${esc(t.code)}" data-season="${esc(String(p.season))}" title="Open the team's injury card">${esc(t.code)}</a> · ${esc(t.name)}`, raw: true, left: true }, { label: 'Conf', value: 'conference', left: true }, { label: 'Elo', value: t => num(t.elo_as_of, 0) },
        ...(r.absences_applied && r.teams.length && 'absence_penalty_today' in r.teams[0] ? [
          { label: `Elo on ${r.as_of}`, value: t => num(t.elo_as_of - t.absence_penalty_today, 0), cls: t => t.absence_penalty_today ? 'bad' : '', title: 'The rating the team plays at on the cutoff date: its Elo less the penalties of the absences that cover that date' },
          { label: 'Out (Elo)', value: t => t.absence_penalty ? `−${num(t.absence_penalty, 0)}` : '', cls: t => t.absence_penalty ? 'bad' : '', title: 'Mean rating penalty over the remaining games from the injury report and the players marked out' }] : []),
        { label: 'Record', value: t => `${t.wins}-${t.losses}` }, { label: 'Exp. wins', value: t => num(t.expected_wins, 1) },
        { label: 'Top 6', value: t => pct(t.p_top6) }, { label: 'Play-in', value: t => pct(t.p_play_in) }, { label: 'Playoffs', value: t => pct(t.p_playoffs) },
        { label: '2nd round', value: t => pct(t.p_second_round) }, { label: 'Conf. finals', value: t => pct(t.p_conf_finals) }, { label: 'Finals', value: t => pct(t.p_finals) },
        { label: 'Title', value: t => `${pct(t.p_champion, 2)} ± ${pct(1.96 * t.p_champion_se, 2)}`, cls: () => 'good' },
        ...(r.held ? [
          heldCol('Out held (Elo)', h => h.absence_penalty ? `−${num(h.absence_penalty, 0)}` : '', 'Mean penalty over the remaining games with the absences covering the cutoff date held to the last regular-season game'),
          heldCol('Exp. wins held', h => num(h.expected_wins, 1), 'Expected wins if the roster of the cutoff date were the roster all season'),
          heldCol('Playoffs held', h => pct(h.p_playoffs), 'Playoff chance with the absences of the cutoff date held all season'),
          heldCol('Title held', h => pct(h.p_champion, 2), 'Title chance with the absences of the cutoff date held all season')] : []),
        { label: 'Actual', value: t => t.actual_result ? RESULT[t.actual_result] + (t.actual_seed ? ` (${t.actual_seed})` : '') : '–', left: true, cls: t => t.actual_result === 'champion' ? 'good' : '' }];
      document.getElementById('results').innerHTML = table(cols, r.teams);
      renderProjected(r); renderCup(r);
      const top = r.teams.slice(0, 16);
      chart('champChart', { type: 'bar', data: { labels: top.map(t => t.code), datasets: [{ label: 'P(champion)', data: top.map(t => t.p_champion), backgroundColor: '#2563eb' }] }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      const all = r.teams.slice().sort((a, b) => b.p_playoffs - a.p_playoffs);
      chart('stageChart', { type: 'bar', data: { labels: all.map(t => t.code), datasets: [{ label: 'Playoffs', data: all.map(t => t.p_playoffs), backgroundColor: '#2563eb' }, { label: 'Play-in', data: all.map(t => t.p_play_in), backgroundColor: '#f59e0b' }, { label: 'Conf. finals', data: all.map(t => t.p_conf_finals), backgroundColor: '#16a34a' }] }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) }, max: 1 } } } });
      setStatus(status, `done · ${job.model_version}`);
    } catch (e) { setStatus(status, e.message, true); } finally { btn.disabled = false; }
  });
})();
