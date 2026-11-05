// The Tonight page: the next night's games at the ratings the teams play at, beside the
// sportsbook's lines, the bets the edge rule flags, and every team's rating tonight.
(async () => {
  const { pct, num, esc, table, getJSON, labelOf, teamLink, bonusText, whatIfSentence, query, homeBonusNote, playerSearch, whatIfControl } = NBA;
  const seasonSelect = document.getElementById('season');
  // One team's spread as a sportsbook writes it (the points it gets, + or −, from its expected
  // margin), and an American line with its sign.
  const teamSpread = m => Math.abs(m) < 0.25 ? 'pick' : `${m > 0 ? '−' : '+'}${num(Math.abs(m), 1)}`;
  const line = o => (o > 0 ? '+' : '−') + Math.round(Math.abs(o));
  const search = playerSearch(document.getElementById('whoOptions'), document.getElementById('wiStatus'));
  const whatIfs = whatIfControl(search, () => loadTonight());
  const homeQuery = () => { const v = document.getElementById('radv').value.trim(); return query(v === '' ? '' : `home_advantage=${encodeURIComponent(v)}`, whatIfs.param()); };
  // The bar a bet's expected return must clear to be flagged, as the field gives it in percent.
  const minReturn = () => { const field = document.getElementById('minret'); const v = parseFloat(field.value); if (Number.isFinite(v) && v >= 0 && v <= 100) return v / 100; field.value = '5'; return 0.05; };
  const minReturnQuery = () => query(`min_return=${encodeURIComponent(minReturn())}`, whatIfs.param());
  const statusNames = { out: 'out', out_for_season: 'out for season', absent: 'absent', doubtful: 'doubtful', questionable: 'questionable', day_to_day: 'day-to-day', probable: 'probable', available: 'available', unknown: 'listed' };
  let tonightRequest = 0;
  let marketSeen = { key: null, at: 0, market: null };
  async function loadTonight() {
    const season = parseInt(seasonSelect.value);
    const note = document.getElementById('tonightNote');
    const mine = ++tonightRequest;
    try {
      const t = await getJSON(`/api/seasons/${season}/tonight${homeQuery()}`);
      if (mine !== tonightRequest) return;
      // The night may not be today: an off day, a break, or every game of today played.
      const when = t.night === t.as_of ? 'tonight' : `on ${t.night}`;
      const whatIfText = whatIfSentence(t.what_ifs);
      note.textContent = t.note ? t.note : t.night
        ? `${labelOf(t.season)}: ${t.games.length} game${t.games.length === 1 ? '' : 's'} ${t.games_played ? `still to play of ${t.games.length + t.games_played} ` : ''}on ${t.night}${t.night === t.as_of ? ' (today)' : ', the next night with a game'}, read as of ${t.as_of} with the report of ${t.report_date}. ${bonusText(t.home_advantage, t.home_advantage_fitted)}. Ratings are at full strength: ${t.games_played_short ? `${t.games_played_short} team game${t.games_played_short === 1 ? '' : 's'} played short this season are taken out of them` : 'no game played short is on record, so they equal the stored Elo'} (${t.rating_model}).${whatIfText}`
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
          ? `<p class="muted" style="margin:.5rem 0 0">Bets the model likes at a bar of ${esc(bar)} of the stake (its chance of winning times the payout at ${esc(book)}'s price, less its chance of losing; the price carries the margin, so the bar is above it), best first:</p><ul>${picks.map(p => `<li>${esc(p.text)}</li>`).join('')}</ul><p class="muted" style="margin:.3rem 0 0">The edge is the model's disagreement with ${esc(book)}, which is unproven until the <a href="/market">Model vs Market</a> page's flagged record says otherwise. ${esc(basis)}</p>`
          : `<p class="muted" style="margin:.5rem 0 0">No bet reaches the bar of ${esc(bar)} of the stake at ${esc(book)}'s prices. ${esc(basis)}</p>`;
      };
      // The lines do not depend on the home bonus: a market read for this season, night and
      // bar in the last minute is reused rather than asked for again (the server keeps the
      // lines longer).
      const key = `${season}|${t.night}|${minReturn()}|${whatIfs.param()}`;
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
    } catch (e) { if (mine !== tonightRequest) return; note.textContent = `Tonight unavailable: ${e.message}`; document.getElementById('tonightGames').innerHTML = ''; document.getElementById('tonightTeams').innerHTML = ''; document.getElementById('tonightMarketNote').textContent = ''; document.getElementById('tonightPicks').innerHTML = ''; document.getElementById('tonightLegendBox').hidden = true; }
  }
  seasonSelect.addEventListener('change', loadTonight);
  document.getElementById('minret').addEventListener('change', loadTonight);
  document.getElementById('radv').addEventListener('change', loadTonight);
  homeBonusNote(seasonSelect, document.getElementById('radv'), document.getElementById('advNote'));
  loadTonight();
})();
