// Top 100: the career worth ranking in tiers, from the stored games and box scores.
(async () => {
  const { num, pct, esc, table, cards, getJSON, labelOf, setStatus } = NBA;
  const el = id => document.getElementById(id);
  const status = el('status');
  const TIERS = [
    'More likely than not one of the best 10',
    'More likely than not one of the best 25',
    'More likely than not one of the best 50',
    'More likely than not one of the best 100',
    'Less than an even chance of the best 100',
  ];
  const seasons = p => `${labelOf(p.first_season)}–${labelOf(p.last_season)}`;
  const pm = (x, e) => `${num(x, 0)} ± ${num(e, 0)}`;
  const note = p => !p.gap ? 'No game missed inside a stint: the box score alone'
    : p.gap.dominant_season ? `${labelOf(p.gap.dominant_season)} carries over a third of the weight of the gap` : '';
  const who = [
    { label: 'Player', value: 'name', left: true },
    { label: 'Career', value: seasons, left: true, title: 'First and last season as a regular, from 1976-77' },
  ];
  const parts = [
    { label: 'Worth', value: p => pm(p.worth, p.error), title: 'Elo: the box score and the games together, each weighted by its precision, with the standard error. The ranking draws from this.' },
    { label: 'Box score', value: p => pm(p.prior, p.prior_error), title: 'Elo: what the box score of his career says the gap is, from the line fitted over all the players, with its error' },
    { label: 'Games', value: p => p.gap ? pm(p.gap.worth, p.gap.error) : '', title: 'Elo: the gap between his teams\' adjusted margins in the games he played and the games he missed, with its standard error' },
  ];
  const evidence = [
    { label: 'Played', value: p => p.career_games.toLocaleString(), title: 'Games played as a regular, over every stint' },
    { label: 'Missed', value: p => p.gap ? p.gap.missed : 0, title: 'Team games missed between his first game and his last of a stint' },
  ];
  const ranked = [
    { label: '#', value: 'rank', title: 'The place among the ranked players: by tier, then by expected rank' },
    ...who, ...parts,
    { label: 'Exp. rank', value: p => num(p.expected_rank, 0), title: 'His mean place over the draws, among every ranked player' },
    { label: 'Top 10', value: p => pct(p.top10, 0), title: 'The share of the draws that put him in the best 10' },
    { label: 'Top 25', value: p => pct(p.top25, 0), title: 'The share of the draws that put him in the best 25' },
    { label: 'Top 50', value: p => pct(p.top50, 0), title: 'The share of the draws that put him in the best 50' },
    { label: 'Top 100', value: p => pct(p.top100, 0), title: 'The share of the draws that put him in the best 100' },
    ...evidence,
    { label: 'Note', value: note, left: true },
  ];
  const withTier = [ranked[0], { label: 'Tier', value: 'tier', title: 'The smallest group he is more likely than not to belong to; 5 is none of them' }, ...ranked.slice(1)];
  setStatus(status, 'Computing from the stored games…');
  let r;
  try { r = await getJSON('/api/players/career-worth'); } catch (e) { setStatus(status, e.message, 'error'); return; }
  setStatus(status, '');
  const size = r.settings.listed, minGames = r.settings.min_games;
  const list = r.players.slice(0, size), below = r.players.slice(size);
  const c = r.calibration, half = r.half_check;
  if (!r.ranked || !c) {
    el('note').textContent = 'No player is ranked yet: the ranking needs the box scores of the seasons from 1976-77. Load them on the Data page.';
    return;
  }
  cards(el('cards'), [
    { label: 'Ranked players', value: r.ranked.toLocaleString(), sub: `with ${minGames} games as a regular from 1976-77; ${r.short_careers.toLocaleString()} careers are shorter and are not ranked` },
    { label: 'Typical error', value: `${num(r.typical_error, 0)} Elo`, sub: `against a spread of ${num(r.spread, 0)} Elo between the worths of the ranked players; the games alone give an error of ${num(r.typical_gap_error, 0)}` },
    { label: `Expected in the true best ${size}`, value: `${num(r.expected_in_list, 0)} of ${list.length}`, sub: 'the sum of the listed players\' chances: the rest are on the list by luck, and as many better players are off it' },
    { label: 'Box score to games', value: `${num(c.slope, 2)} to 1`, sub: `Elo of gap for each Elo of career box worth, plus ${num(c.intercept, 0)}; the true gaps lie ${num(c.spread, 0)} Elo around that line, over ${c.players} players` },
    ...(half ? [{ label: 'Half-career check', value: `r = ${num(half.blend.correlation, 2)}`, sub: `the worth from half of a player's seasons against the games of the other half; the games alone give ${num(half.gap.correlation, 2)}, the box score alone ${num(half.prior.correlation, 2)}` }] : []),
  ]);
  el('note').textContent = `${r.settings.draws.toLocaleString()} draws. Model ${r.model}.`;
  el('tiers').innerHTML = TIERS.map((text, i) => {
    const rows = list.filter(p => p.tier === i + 1);
    if (!rows.length) return '';
    return `<section class="panel"><h2>Tier ${i + 1} <span class="muted small">${esc(text)} · ${rows.length} ${rows.length === 1 ? 'player' : 'players'}</span></h2><div class="table-wrap">${table(ranked, rows)}</div></section>`;
  }).join('');
  el('belowCount').textContent = `(${below.length})`;
  el('below').innerHTML = table(withTier, below);

  // Find: any ranked player by name, with where he stands.
  const place = p => p.rank <= size ? `On the list at ${p.rank}, tier ${p.tier}`
    : `Ranked ${p.rank} of ${r.ranked}, below the list: a ${pct(p.top100, 0)} chance of the best 100`;
  function find() {
    const q = el('find').value.trim().toLowerCase();
    el('foundPanel').classList.toggle('hidden', q.length < 2);
    if (q.length < 2) return;
    const hits = r.players.filter(p => p.name.toLowerCase().includes(q)).slice(0, 25);
    el('found').innerHTML = hits.length ? table([...who, { label: 'Where', value: place, left: true }, ...parts,
      { label: 'Box worth', value: p => num(p.box_worth, 0), title: 'Elo: the box-score worth of his seasons, weighted by their minutes' },
      ...evidence, { label: 'Note', value: note, left: true }], hits) : '';
    el('foundNote').textContent = hits.length ? '' : `No ranked player has this name. A player with fewer than ${minGames} games as a regular from 1976-77 is not ranked.`;
  }
  el('find').addEventListener('input', find);
  const asked = new URLSearchParams(location.search).get('find');
  if (asked) { el('find').value = asked; find(); }
})();
