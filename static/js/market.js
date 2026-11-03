// Model vs Market: every scored snapshot of a season and the paired verdict.
(async () => {
  const { pct, num, esc, table, cards, getJSON, labelOf, setStatus } = NBA;
  const seasonSel = document.getElementById('season');
  const minret = document.getElementById('minret');
  const status = document.getElementById('status');
  const signed = (x, d = 1) => x == null ? '' : `${x > 0 ? '+' : x < 0 ? '−' : ''}${num(Math.abs(x), d)}`;
  const spread = (code, s) => s == null ? '' : `${code} ${s > 0 ? '+' : s < 0 ? '−' : ''}${num(Math.abs(s), 1)}`;
  const line = o => o == null ? '' : (o > 0 ? `+${num(o, 0)}` : num(o, 0));
  const closer = g => g.brier_market == null ? '' : Math.abs(g.brier_market - g.brier_model) < 1e-12 ? 'Level' : g.brier_model < g.brier_market ? 'Model' : 'Market';
  // The fewest games both priced before a lead is called; the same as the domain's.
  const MIN_GAMES_TO_CALL = 30;
  const bar = () => { const v = parseFloat(minret.value); return Number.isFinite(v) ? `&min_return=${v / 100}` : ''; };
  const sideCode = (g, side) => side === 'home' ? g.snapshot.home_code : g.snapshot.away_code;
  const bet = (side, result, units, g) => side ? `${sideCode(g, side)} ${({ won: 'won', lost: 'lost', push: 'push' })[result] || ''} (${signed(units, 2)})` : '';
  // The verdict in words: who leads, by how much, and whether the lead is more than luck.
  const verdict = (c, book, s) => {
    if (!c) return 'No game both priced has been scored yet.';
    const n = c.games;
    const games = k => `${k} game${k === 1 ? '' : 's'}`;
    const who = c.leader === 'model' ? 'the model' : c.leader === 'market' ? book : null;
    const lead = who
      ? `${who[0].toUpperCase()}${who.slice(1)} has been the closer: a mean Brier score of ${num(c.leader === 'model' ? s.brier_model : s.brier_market, 3)} against ${num(c.leader === 'model' ? s.brier_market : s.brier_model, 3)}, closer on ${c.leader === 'model' ? c.model_closer : c.market_closer} of the ${games(n)}${c.ties ? ` (level on ${c.ties})` : ''}.`
      : `The two are level: a mean Brier score of ${num(s.brier_model, 3)} each over ${games(n)}.`;
    let luck;
    if (!who) luck = '';
    else if (c.z == null) luck = n < 2 ? 'One game says nothing about which is better.' : 'Every game shows the same gap, so its error cannot be measured yet.';
    else if (c.decided) luck = `The lead is ${num(Math.abs(c.z), 1)} standard errors, more than luck alone gives: call it ${who}'s, for now.`;
    else if (n < MIN_GAMES_TO_CALL) luck = `The lead is ${num(Math.abs(c.z), 1)} standard errors, but ${games(n)} ${n === 1 ? 'is' : 'are'} too few to call it: the page calls a lead from ${MIN_GAMES_TO_CALL} games on.${c.games_to_tell ? ` At this gap and spread it takes about ${c.games_to_tell.toLocaleString()} games to tell.` : ' At this gap it would take more than ten seasons of games to tell.'}`;
    else luck = `The lead is ${num(Math.abs(c.z), 1)} standard errors, within what luck alone gives, so the two are not yet apart.${c.games_to_tell ? ` At this gap and spread it takes about ${c.games_to_tell.toLocaleString()} games to tell; ${n.toLocaleString()} so far.` : ' At this gap it would take more than ten seasons of games to tell.'}`;
    return `${lead} ${luck}`.trim();
  };
  let request = 0;
  async function load() {
    const season = parseInt(seasonSel.value);
    const mine = ++request;
    setStatus(status, 'Loading…');
    try {
      const r = await getJSON(`/api/seasons/${season}/market/track?limit=2000${bar()}`);
      if (mine !== request) return;
      setStatus(status, '');
      const providers = [...new Set(r.games.map(g => g.snapshot.provider))];
      const book = providers.length === 1 ? providers[0] : 'the sportsbook';
      const Book = providers.length === 1 ? providers[0] : 'Sportsbook';
      const c = r.comparison;
      const s = r.overall;
      document.getElementById('verdict').textContent = r.note ? '' : verdict(c, book, s);
      document.getElementById('note').textContent = r.note ? r.note : `${labelOf(r.season)}: ${r.snapshots} game${r.snapshots === 1 ? '' : 's'} saved, ${r.scored} scored, ${r.pending} waiting for a result${r.unmatched ? `, ${r.unmatched} matching no stored game (a game moved to another date)` : ''}; ${s.priced} of the scored games carry a sportsbook chance. The model saving now is ${r.current_model}.${r.games.length < r.scored ? ` The table lists the latest ${r.games.length}.` : ''}`;
      cards(document.getElementById('cards'), c ? [
        { label: 'Games both priced', value: c.games.toLocaleString(), sub: `${r.scored.toLocaleString()} scored in all` },
        { label: 'Model closer', value: `${c.model_closer} of ${c.games}`, sub: `Brier ${num(s.brier_model, 3)}, log loss ${num(s.log_loss_model, 3)}` },
        { label: `${Book} closer`, value: `${c.market_closer} of ${c.games}`, sub: `Brier ${num(s.brier_market, 3)}, log loss ${num(s.log_loss_market, 3)}` },
        { label: 'Lead in standard errors', value: c.z == null ? '–' : num(Math.abs(c.z), 1), sub: c.z == null ? (c.leader == null ? 'level' : 'not measurable yet') : c.decided ? `${c.leader === 'model' ? 'the model' : book} leads` : c.games < MIN_GAMES_TO_CALL ? `too few games to call (${MIN_GAMES_TO_CALL} needed)` : 'not yet apart' },
        { label: 'Games to tell', value: c.games_to_tell == null ? '–' : c.games_to_tell.toLocaleString(), sub: c.games_to_tell == null ? (c.leader == null ? 'the two are level' : c.z == null ? 'no spread measured yet' : 'more than ten seasons at this gap') : 'at the gap and spread so far' }] : []);
      const rows = r.scored ? [{ key: 'All games', ...s, comparison: c }]
        .concat(r.by_model.map(g => ({ ...g, key: `Model ${g.key}${g.key === r.current_model ? ' (now)' : ''}` })))
        .concat(r.by_month.map(g => ({ ...g, key: `Month ${g.key}` }))) : [];
      const cmp = (g, f) => g.comparison ? f(g.comparison) : '';
      document.getElementById('summary').innerHTML = rows.length ? table([
        { label: 'Group', value: g => g.key, left: true },
        { label: 'Scored', value: g => g.games },
        { label: 'Both priced', value: g => g.priced, title: 'Games with a sportsbook chance; the columns to the right compare these' },
        { label: 'Model closer', value: g => cmp(g, c => c.model_closer), cls: g => g.comparison && g.comparison.model_closer > g.comparison.market_closer ? 'good' : '' },
        { label: `${Book} closer`, value: g => cmp(g, c => c.market_closer) },
        { label: 'Brier, model', value: g => g.priced ? num(g.brier_model, 3) : '', cls: g => g.priced && g.brier_model < g.brier_market ? 'good' : '', title: 'Mean squared error of the home chance against the result; lower is better' },
        { label: `Brier, ${Book}`, value: g => g.priced ? num(g.brier_market, 3) : '' },
        { label: 'Log loss, model', value: g => g.priced ? num(g.log_loss_model, 3) : '', cls: g => g.priced && g.log_loss_model < g.log_loss_market ? 'good' : '', title: 'Mean of minus the log of the chance given to what happened; lower is better' },
        { label: `Log loss, ${Book}`, value: g => g.priced ? num(g.log_loss_market, 3) : '' },
        { label: 'Lead (SE)', value: g => cmp(g, c => c.z == null ? '' : num(Math.abs(c.z), 1)), title: `The mean Brier gap in standard errors; two and over from ${MIN_GAMES_TO_CALL} games on is more than luck alone gives` },
        { label: 'Leads', value: g => cmp(g, c => c.leader == null ? '' : c.leader === 'model' ? 'Model' : Book), left: true, cls: g => g.comparison && g.comparison.decided ? 'good' : 'muted' },
        { label: 'Lean W-L', value: g => `${g.lean_won}-${g.lean_lost}`, title: 'The side whose chance the model put above the sportsbook\'s, bet on the moneyline' },
        { label: 'Lean per bet', value: g => (g.lean_won + g.lean_lost) ? signed(g.lean_return, 2) : '', cls: g => g.lean_return > 0 ? 'good' : g.lean_return < 0 ? 'bad' : '' },
        { label: 'Flagged W-L-P', value: g => `${g.flagged_won}-${g.flagged_lost}${g.flagged_push ? `-${g.flagged_push}` : ''}`, title: `The bets the edge rule flagged at the bar of ${num(100 * r.min_return, 0)}% of the stake` },
        { label: 'Flagged per bet', value: g => (g.flagged_won + g.flagged_lost + g.flagged_push) ? signed(g.flagged_return, 2) : '', cls: g => g.flagged_return > 0 ? 'good' : g.flagged_return < 0 ? 'bad' : '' }], rows) : '';
      document.getElementById('games').innerHTML = r.games.length ? table([
        { label: 'Date', value: g => g.snapshot.game_date, left: true },
        { label: 'Game', value: g => `${g.snapshot.away_code} @ ${g.snapshot.home_code}`, left: true },
        { label: 'Final', value: g => `${g.snapshot.away_code} ${g.away_score}, ${g.snapshot.home_code} ${g.home_score}`, left: true },
        { label: 'Model, home wins', value: g => pct(g.snapshot.p_home_model, 0), title: 'The model\'s chance at the ratings the teams played at' },
        { label: `${Book}, home wins`, value: g => g.snapshot.p_home_market == null ? '' : pct(g.snapshot.p_home_market, 0), title: 'The sportsbook\'s chance with its margin taken out' },
        { label: 'Closer', value: closer, left: true, cls: g => closer(g) === 'Model' ? 'good' : '' },
        { label: 'Model spread', value: g => spread(g.snapshot.home_code, -g.snapshot.home_margin_model), title: 'The model\'s expected home margin as a spread' },
        { label: `${Book} spread`, value: g => spread(g.snapshot.home_code, g.snapshot.home_spread) },
        { label: 'Margin', value: g => signed(g.home_score - g.away_score, 0), title: 'The final margin by the home team' },
        { label: `${Book} moneylines`, value: g => g.snapshot.home_moneyline == null ? '' : `${g.snapshot.away_code} ${line(g.snapshot.away_moneyline)} / ${g.snapshot.home_code} ${line(g.snapshot.home_moneyline)}`, left: true },
        { label: 'Lean', value: g => bet(g.lean, g.lean_result, g.lean_units, g), left: true, cls: g => g.lean_units > 0 ? 'good' : g.lean_units < 0 ? 'bad' : '' },
        { label: 'Flagged', value: g => g.flagged.map(f => `${sideCode(g, f.edge.side)} ${f.edge.kind === 'spread' ? `${f.edge.line > 0 ? '+' : f.edge.line < 0 ? '−' : ''}${num(Math.abs(f.edge.line), 1)} ` : ''}${line(f.edge.price)} ${f.result} (${signed(f.units, 2)})`).join('; '), left: true },
        { label: 'Model version', value: g => g.snapshot.model_version, left: true }], r.games) : '';
    } catch (e) {
      if (mine !== request) return;
      setStatus(status, `Unavailable: ${e.message}`, true);
      document.getElementById('verdict').textContent = '';
      document.getElementById('note').textContent = '';
      document.getElementById('cards').innerHTML = '';
      document.getElementById('summary').innerHTML = '';
      document.getElementById('games').innerHTML = '';
    }
  }
  seasonSel.addEventListener('change', load);
  minret.addEventListener('change', load);
  load();
})();
