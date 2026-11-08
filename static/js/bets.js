// Bets: a pot staked on every game's favourite, as a portfolio, from the market record.
(async () => {
  const { num, pct, table, cards, getJSON, labelOf, setStatus, chart, color, dayOf, dateOf, dateScale } = NBA;
  const el = id => document.getElementById(id);
  const status = el('status');
  const money = x => x == null ? '–' : `${x < 0 ? '−' : ''}$${Math.abs(x).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
  const signed = x => x == null ? '–' : `${x > 0 ? '+' : x < 0 ? '−' : ''}$${Math.abs(x).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
  const strategies = book => ({ market_favourite: `${book} favourite`, model_favourite: 'Model favourite', model_disagreements: 'Model where it disagrees' });
  let request = 0;
  async function load() {
    const mine = ++request;
    setStatus(status, 'Loading…');
    try {
      const q = new URLSearchParams();
      if (el('from').value) q.set('from', el('from').value);
      q.set('pot', el('pot').value); q.set('stake', el('stake').value);
      const r = await getJSON(`/api/market/bets?${q}`);
      if (mine !== request) return;
      setStatus(status, '');
      const p = r.portfolio;
      const book = r.providers.length === 1 ? r.providers[0] : 'Sportsbook';
      const label = strategies(book);
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
        sub: `${signed(x.net)} (${pct(x.return_share)}) on ${x.bets.toLocaleString()} bets · peak ${money(x.peak)} · drawdown ${money(x.max_drawdown)} (${pct(x.max_drawdown_share)})`,
      })));
      const days = p.days.map(d => dayOf(d.date));
      chart('potChart', { type: 'line', data: { datasets: runs.map((x, k) => ({
          label: label[x.run.strategy], data: p.days.map(d => ({ x: dayOf(d.date), y: d.values[x.i] })),
          borderColor: color(k), backgroundColor: color(k), borderWidth: 1.5, pointRadius: 0, pointHitRadius: 6, tension: 0 })) },
        options: { responsive: true, maintainAspectRatio: false, parsing: false, animation: false, interaction: { mode: 'nearest', axis: 'x', intersect: false },
          plugins: { legend: { display: true }, tooltip: { callbacks: { title: items => items.length ? dateOf(items[0].raw.x) : '', label: item => `${item.dataset.label}: ${money(item.raw.y)}` } } },
          scales: { x: dateScale(days), y: { title: { display: true, text: `Pot ($), stake ${money(p.settings.stake)} a game` }, ticks: { callback: v => money(v) } } } } });
      // The verdict: each run's net in standard errors, so a reader knows what is luck.
      el('verdict').textContent = runs.map(x => {
        const z = x.net_se ? Math.abs(x.net) / x.net_se : null;
        const luck = z == null ? '' : z >= 2 ? `, ${num(z, 1)} standard errors ${x.net < 0 ? 'below' : 'above'} zero, more than luck alone gives` : `, ${num(z, 1)} standard error${z === 1 ? '' : 's'} from zero, within what luck alone gives`;
        return `${label[x.run.strategy]}: ${signed(x.net)} over ${x.bets.toLocaleString()} bets${luck}.`;
      }).join(' ');
      const margin = p.mean_margin == null ? '' : ` The sportsbook's mean margin on these games is ${pct(p.mean_margin, 2)}; at the posted prices it costs about ${money(p.mean_margin * p.settings.stake * p.games)} over the run.`;
      el('note').textContent = `${p.games.toLocaleString()} games bet on from ${p.first} to ${p.last}, over ${r.seasons.length} season${r.seasons.length === 1 ? '' : 's'} with a record (${r.scored.toLocaleString()} scored games in all${p.before_start ? `, ${p.before_start.toLocaleString()} before the start date` : ''}${p.unpriced ? `, ${p.unpriced} without a sportsbook price` : ''}${r.pending ? `, ${r.pending} waiting for a result` : ''}${r.unmatched ? `, ${r.unmatched} matching no stored game` : ''}). Lines from ${r.providers.join(', ')}.${margin}`;
      const at = (row, strategy, pr) => row.runs[p.runs.findIndex(x => x.run.strategy === strategy && x.run.price === pr)];
      const rows = p.seasons.map(s => ({ label: labelOf(s.season), ...s })).concat([{ label: 'All', games: p.games, runs: p.runs.map(x => ({ bets: x.bets, wins: x.wins, net: x.net })), total: true }]);
      el('seasons').innerHTML = table([
        { label: 'Season', value: 'label', left: true }, { label: 'Games', value: r => r.games.toLocaleString() },
        { label: `${book} fav. won`, value: r => at(r, 'market_favourite', 'posted').wins.toLocaleString() },
        { label: 'Net', value: r => signed(at(r, 'market_favourite', 'posted').net), cls: r => at(r, 'market_favourite', 'posted').net < 0 ? 'bad' : 'good' },
        { label: 'Net fair', value: r => signed(at(r, 'market_favourite', 'fair').net), cls: r => at(r, 'market_favourite', 'fair').net < 0 ? 'bad' : 'good' },
        { label: 'Model fav. won', value: r => at(r, 'model_favourite', 'posted').wins.toLocaleString() },
        { label: 'Net', value: r => signed(at(r, 'model_favourite', 'posted').net), cls: r => at(r, 'model_favourite', 'posted').net < 0 ? 'bad' : 'good' },
        { label: 'Net fair', value: r => signed(at(r, 'model_favourite', 'fair').net), cls: r => at(r, 'model_favourite', 'fair').net < 0 ? 'bad' : 'good' },
        { label: 'Disagreements', value: r => at(r, 'model_disagreements', 'posted').bets.toLocaleString() },
        { label: 'Model right', value: r => at(r, 'model_disagreements', 'posted').wins.toLocaleString() },
        { label: 'Net', value: r => signed(at(r, 'model_disagreements', 'posted').net), cls: r => at(r, 'model_disagreements', 'posted').net < 0 ? 'bad' : 'good' },
        { label: 'Net fair', value: r => signed(at(r, 'model_disagreements', 'fair').net), cls: r => at(r, 'model_disagreements', 'fair').net < 0 ? 'bad' : 'good' }], rows);
    } catch (e) { if (mine === request) setStatus(status, e.message, true); }
  }
  for (const id of ['from', 'pot', 'stake', 'price']) el(id).addEventListener('change', load);
  load();
})();
