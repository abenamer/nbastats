(async () => {
  const { pct, num, esc, getJSON, runJob, table, setStatus, chart, RESULT } = NBA;
  const status = document.getElementById('status');
  let bracket = null, overrides = {};
  const ROUND = { first: '1st round', second: '2nd round', conf_finals: 'Conf. finals', finals: 'Finals' };
  function renderBracket(b, exact) {
    const slots = b.teams, byPos = [[0, 1], [2, 3], [4, 5], [6, 7]];
    const conf = (ci, name) => `<div class="conf"><h4>${name}</h4>` + byPos.map(([p, q]) => {
      const a = slots[ci * 8 + p], c = slots[ci * 8 + q];
      const s = b.actual_series.find(x => (x.slot_a === ci * 8 + p && x.slot_b === ci * 8 + q) || (x.slot_b === ci * 8 + p && x.slot_a === ci * 8 + q));
      const w = (t, slot) => `<div class="t ${s && s.winner_slot === slot ? 'w' : ''}"><span>${t.seed} ${esc(t.code)} (${t.wins}-${t.losses})</span><span>${num(t.elo, 0)}${exact ? ' · ' + pct(exact[slot][3]) : ''}</span></div>`;
      return `<div class="series">${w(a, ci * 8 + p)}${w(c, ci * 8 + q)}<div class="round">${s ? `actual ${s.wins_a}-${s.wins_b}` : 'not played'}</div></div>`;
    }).join('') + '</div>';
    document.getElementById('bracket').innerHTML = conf(0, 'East') + conf(1, 'West');
    document.getElementById('notes').innerHTML = (b.notes || []).map(esc).join('<br>');
  }
  async function loadBracket() {
    const season = document.getElementById('season').value;
    setStatus(status, 'Loading bracket…');
    try {
      const d = await getJSON(`/api/seasons/${season}/bracket?home_advantage=${document.getElementById('adv').value}&scale=${document.getElementById('scale').value}`);
      bracket = d.bracket; overrides = {};
      document.getElementById('cutoff').textContent = `Information cutoff: ${d.information_cutoff} (ratings after the last game before the playoffs). ${d.complete ? 'Postseason complete.' : 'Postseason incomplete in the data.'}`;
      renderBracket(d.bracket, d.exact);
      renderTable(d.bracket.teams.map((t, i) => ({ ...t, exact: d.exact ? d.exact[i] : null })), null);
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
  }
  function renderTable(rows, sim) {
    const cols = [
      { label: 'Team', value: r => `${r.conference === 'East' ? 'E' : 'W'}${r.seed} ${r.code} · ${r.name}`, left: true }, { label: 'Record', value: r => `${r.wins}-${r.losses}` },
      { label: 'Elo', value: r => `<input type="number" data-code="${r.code}" value="${num(r.elo, 1)}" step="1" style="width:5.5rem">`, raw: true },
      { label: 'Exact 2nd rd', value: r => r.exact ? pct(r.exact[0]) : '–' }, { label: 'Exact conf. finals', value: r => r.exact ? pct(r.exact[1]) : '–' },
      { label: 'Exact Finals', value: r => r.exact ? pct(r.exact[2]) : '–' }, { label: 'Exact title', value: r => r.exact ? pct(r.exact[3], 2) : '–', cls: () => 'good' }];
    if (sim) cols.push({ label: 'MC title', value: r => `${pct(r.p_champion, 2)} ± ${pct(1.96 * r.p_champion_se, 2)}` }, { label: 'MC Finals', value: r => pct(r.p_finals) }, { label: 'Titles (sim)', value: r => r.titles.toLocaleString() });
    cols.push({ label: 'Actual', value: r => r.actual_result ? RESULT[r.actual_result] : '–', left: true, cls: r => r.actual_result === 'champion' ? 'good' : '' });
    document.getElementById('results').innerHTML = table(cols, rows.slice().sort((a, b) => ((b.exact ? b.exact[3] : b.p_champion || 0) - (a.exact ? a.exact[3] : a.p_champion || 0))));
    document.querySelectorAll('#results input[data-code]').forEach(i => i.addEventListener('change', e => { overrides[e.target.dataset.code] = parseFloat(e.target.value); }));
  }
  function modelSpec() {
    const m = document.getElementById('model').value, k = document.getElementById('k').value;
    if (m === 'elo') return { model: 'elo', home_advantage: parseFloat(document.getElementById('adv').value), scale: parseFloat(document.getElementById('scale').value), k: k === '' ? null : parseFloat(k) };
    if (m === 'seed') return { model: 'seed', p_better: 0.75 };
    return { model: m };
  }
  document.getElementById('run').addEventListener('click', async () => {
    const btn = document.getElementById('run'); btn.disabled = true;
    const p = { season: parseInt(document.getElementById('season').value), iterations: parseInt(document.getElementById('iterations').value), seed: parseInt(document.getElementById('seed').value) || 0, model: modelSpec(), elo_source: document.getElementById('eloSource').value, elo_overrides: overrides };
    const name = document.getElementById('name').value.trim(); if (name) p.name = name;
    try {
      const job = await runJob('/api/jobs/playoff', p, j => setStatus(status, `${j.status} ${pct(j.progress, 0)}`));
      const r = job.result; bracket = r.bracket;
      renderBracket(r.bracket, r.simulation.outcomes.map(o => o.exact || [o.p_second_round, o.p_conf_finals, o.p_finals, o.p_champion]));
      renderTable(r.simulation.outcomes, true);
      const top = r.simulation.outcomes.slice().sort((a, b) => b.p_champion - a.p_champion);
      chart('champChart', { type: 'bar', data: { labels: top.map(o => o.code), datasets: [{ label: 'Monte Carlo', data: top.map(o => o.p_champion), backgroundColor: '#2563eb' }, ...(top[0].exact ? [{ label: 'Exact', data: top.map(o => o.exact[3]), backgroundColor: '#9ca3af' }] : [])] }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
      const names = r.bracket.teams;
      document.getElementById('finals').innerHTML = table([{ label: 'East', value: m => names[m[0]].code, left: true }, { label: 'West', value: m => names[m[1]].code, left: true }, { label: 'Share', value: m => pct(m[2] / r.simulation.iterations) }], r.simulation.finals_matchups.slice(0, 12));
      setStatus(status, `${r.simulation.iterations.toLocaleString()} simulations in ${r.simulation.elapsed_ms} ms · actual champion ${r.actual_champion || '–'} · ${job.model_version}`);
    } catch (e) { setStatus(status, e.message, true); } finally { btn.disabled = false; }
  });
  document.getElementById('season').addEventListener('change', loadBracket);
  loadBracket();
})();
