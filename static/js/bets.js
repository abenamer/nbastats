// Bets: a pot staked on every game, as a portfolio, from the market record.
(async () => {
  const { num, pct, table, cards, getJSON, labelOf, setStatus, chart, color, dayOf, dateOf, dateScale } = NBA;
  const el = id => document.getElementById(id);
  const status = el('status');
  const money = x => x == null ? '–' : `${x < 0 ? '−' : ''}$${Math.abs(x).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
  const signed = x => x == null ? '–' : `${x > 0 ? '+' : x < 0 ? '−' : ''}$${Math.abs(x).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
  const labels = book => ({ market_favourite: `${book} favourite`, model_favourite: 'Model favourite', model_disagreements: 'Model where it disagrees', lean_linear: 'Model lean, stake by the gap', lean_square: 'Model lean, stake by the gap squared' });
  const sized = s => s === 'lean_linear' || s === 'lean_square';
  let request = 0;
  async function load() {
    const mine = ++request;
    setStatus(status, 'Loading…');
    try {
      const q = new URLSearchParams();
      if (el('from').value) q.set('from', el('from').value);
      q.set('pot', el('pot').value); q.set('stake', el('stake').value); q.set('edge', el('edge').value);
      const r = await getJSON(`/api/market/bets?${q}`);
      if (mine !== request) return;
      setStatus(status, '');
      const p = r.portfolio;
      // The sportsbook named is the one most of the lines come from; the note says the rest.
      const book = r.providers.length ? r.providers[0].name : 'Sportsbook';
      const label = labels(book);
      const price = el('price').value;
      // The start date field shows the date the run started from, and bounds the pick.
      if (p.first && !el('from').value) el('from').value = p.first;
      if (p.first) el('from').min = p.first;
      if (p.last) el('from').max = p.last;
      if (r.note || !p.games) {
        el('verdict').textContent = '';
        el('note').textContent = r.note || 'No game to bet on from this date.';
        cards(el('cards'), []); el('seasons').innerHTML = '';
        chart('potChart', { type: 'line', data: { datasets: [] } });
        return;
      }
      const runs = p.runs.map((run, i) => ({ ...run, i })).filter(x => x.run.price === price);
      cards(el('cards'), runs.map(x => ({
        label: label[x.run.strategy],
        value: money(x.final_value),
        sub: `${signed(x.net)} (${pct(x.return_share)}) on ${x.bets.toLocaleString()} bets, ${money(x.staked)} staked${sized(x.run.strategy) ? `, largest ${money(x.max_stake)}` : ''} · peak ${money(x.peak)} · drawdown ${money(x.max_drawdown)} (${pct(x.max_drawdown_share)})`,
      })));
      const days = p.days.map(d => dayOf(d.date));
      chart('potChart', { type: 'line', data: { datasets: runs.map((x, k) => ({
          label: label[x.run.strategy], data: p.days.map(d => ({ x: dayOf(d.date), y: d.values[x.i] })),
          borderColor: color(k), backgroundColor: color(k), borderWidth: 1.5, pointRadius: 0, pointHitRadius: 6, tension: 0 })) },
        options: { responsive: true, maintainAspectRatio: false, parsing: false, animation: false, interaction: { mode: 'nearest', axis: 'x', intersect: false },
          plugins: { legend: { display: true }, tooltip: { callbacks: { title: items => items.length ? dateOf(items[0].raw.x) : '', label: item => `${item.dataset.label}: ${money(item.raw.y)}` } } },
          scales: { x: dateScale(days), y: { title: { display: true, text: `Pot ($), base stake ${money(p.settings.stake)} a game` }, ticks: { callback: v => money(v) } } } } });
      // The verdict: each run's net in standard errors, so a reader knows what is luck.
      el('verdict').textContent = runs.map(x => {
        const z = x.net_se ? Math.abs(x.net) / x.net_se : null;
        const luck = z == null ? '' : z >= 2 ? `, ${num(z, 1)} standard errors ${x.net < 0 ? 'below' : 'above'} zero, more than luck alone gives` : `, ${num(z, 1)} standard error${z === 1 ? '' : 's'} from zero, within what luck alone gives`;
        return `${label[x.run.strategy]}: ${signed(x.net)} over ${x.bets.toLocaleString()} bets${luck}.`;
      }).join(' ');
      const margin = p.mean_margin == null ? '' : ` The sportsbook's mean margin on these games is ${pct(p.mean_margin, 2)}; on a flat stake at the posted prices it costs about ${money(p.mean_margin * p.settings.stake * p.games)} over the run.`;
      const others = r.providers.slice(1);
      const lines = r.providers.length ? ` Lines from ${book} on ${r.providers[0].games.toLocaleString()} games${others.length ? `, the rest from ${others.length === 1 ? others[0].name : `${others.length} other sportsbooks`} where it had none` : ''}.` : '';
      el('note').textContent = `${p.games.toLocaleString()} games bet on from ${p.first} to ${p.last}, over ${r.seasons.length} season${r.seasons.length === 1 ? '' : 's'} with a record (${r.scored.toLocaleString()} scored games in all${p.before_start ? `, ${p.before_start.toLocaleString()} before the start date` : ''}${p.unpriced ? `, ${p.unpriced} without a sportsbook price` : ''}${r.pending ? `, ${r.pending} waiting for a result` : ''}${r.unmatched ? `, ${r.unmatched} matching no stored game` : ''}). The sized strategies lay the base stake at a gap of ${num(p.settings.edge * 100, 1)} points.${lines}${margin}`;
      const at = (row, strategy) => row.runs[p.runs.findIndex(x => x.run.strategy === strategy && x.run.price === price)];
      const net = strategy => ({ label: 'Net', value: r => signed(at(r, strategy).net), cls: r => at(r, strategy).net < 0 ? 'bad' : 'good' });
      const rows = p.seasons.map(s => ({ label: labelOf(s.season), ...s })).concat([{ label: 'All', games: p.games, runs: p.runs.map(x => ({ bets: x.bets, wins: x.wins, staked: x.staked, net: x.net })) }]);
      el('seasons').innerHTML = table([
        { label: 'Season', value: 'label', left: true }, { label: 'Games', value: r => r.games.toLocaleString() },
        { label: `${book} fav. won`, value: r => at(r, 'market_favourite').wins.toLocaleString() }, net('market_favourite'),
        { label: 'Model fav. won', value: r => at(r, 'model_favourite').wins.toLocaleString() }, net('model_favourite'),
        { label: 'Disagree', value: r => at(r, 'model_disagreements').bets.toLocaleString(), title: 'Games where the model\'s favourite is not the sportsbook\'s, bet on the model\'s side' }, { label: 'Right', value: r => at(r, 'model_disagreements').wins.toLocaleString() }, net('model_disagreements'),
        { label: 'Lean bets', value: r => at(r, 'lean_linear').bets.toLocaleString(), title: 'Games the model leaned on either way' },
        { label: 'Staked, gap', value: r => money(at(r, 'lean_linear').staked), title: 'Money laid with the stake in proportion to the gap' }, net('lean_linear'),
        { label: 'Staked, gap²', value: r => money(at(r, 'lean_square').staked), title: 'Money laid with the stake as the square of the gap' }, net('lean_square')], rows);
    } catch (e) { if (mine === request) setStatus(status, e.message, true); }
  }
  for (const id of ['from', 'pot', 'stake', 'edge', 'price']) el(id).addEventListener('change', load);
  load();
})();
