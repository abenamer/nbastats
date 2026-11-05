// The What if page: a real season run twice from the same date with the same seed, plain and
// with the reader's changes (ratings set by hand, teams barred from the postseason, the
// format, players out, worth what-ifs), and what moved between the two.
(async () => {
  const { pct, num, esc, runJob, table, setStatus, bracketHtml, homeBonusNote, playerSearch, whatIfControl, absenceControl } = NBA;
  const EXPANSION_ELO = 1300;
  const status = document.getElementById('status');
  const seasonSelect = document.getElementById('season');
  const loadBtn = document.getElementById('load'), runBtn = document.getElementById('run');
  document.getElementById('preseason').addEventListener('change', e => document.getElementById('asOf').disabled = e.target.checked);
  document.getElementById('asOf').disabled = true;
  homeBonusNote(seasonSelect, null, document.getElementById('advNote'));
  const search = playerSearch(document.getElementById('whoOptions'), document.getElementById('whoStatus'));
  const whatIfs = whatIfControl(search, () => {});
  const absences = absenceControl(seasonSelect, search, () => document.getElementById('preseason').checked ? '' : document.getElementById('asOf').value);
  // The plain run and the teams it lists, by team id.
  let base = null;
  const settings = () => {
    const p = { season: parseInt(seasonSelect.value), iterations: parseInt(document.getElementById('iterations').value), seed: parseInt(document.getElementById('seed').value) || 0, home_advantage: 100, regular_season_k: 20, playoff_scale: 1 };
    if (!document.getElementById('preseason').checked && document.getElementById('asOf').value) p.as_of = document.getElementById('asOf').value;
    if (document.getElementById('injuries').checked) p.use_injury_report = true;
    return p;
  };
  const changes = () => {
    const c = { rating_overrides: [], excluded_teams: [] };
    document.querySelectorAll('#teams input[data-rating]').forEach(i => {
      const v = parseFloat(i.value);
      if (Number.isFinite(v) && Math.abs(v - parseFloat(i.dataset.own)) >= 0.5) c.rating_overrides.push({ team_id: Number(i.dataset.rating), elo: v });
    });
    document.querySelectorAll('#teams input[data-bar]:checked').forEach(i => c.excluded_teams.push(Number(i.dataset.bar)));
    const q = document.getElementById('qualification').value; if (q) c.qualification = q;
    const g = document.getElementById('firstRound').value; if (g) c.first_round_games = parseInt(g);
    if (absences.entries.length) c.absences = absences.params();
    if (whatIfs.entries.length) c.worth_overrides = whatIfs.overrides();
    return c;
  };
  function renderTeams(r) {
    document.getElementById('teams').innerHTML = table([
      { label: 'Team', value: t => `${t.code} · ${t.name}`, left: true },
      { label: 'Conf', value: 'conference', left: true },
      { label: 'Record', value: t => `${t.wins}-${t.losses}` },
      { label: `Elo on ${r.as_of}`, value: t => num(t.elo_as_of, 0), title: 'The rating the plain run starts from' },
      { label: 'Exp. wins', value: t => num(t.expected_wins, 1) },
      { label: 'Playoffs', value: t => pct(t.p_playoffs) },
      { label: 'Title', value: t => pct(t.p_champion, 1) },
      { label: 'Rating set to', value: t => `<input type="number" data-rating="${t.team_id}" data-own="${Math.round(t.elo_as_of)}" value="${Math.round(t.elo_as_of)}" min="800" max="2500" step="10" style="width:6rem">`, raw: true, title: 'A rating that replaces the team\'s own at the cutoff; a rating left as shown sets nothing' },
      { label: 'Barred', value: t => `<input type="checkbox" data-bar="${t.team_id}">`, raw: true, title: 'Out of the play-in and the playoffs; the next team takes its place' },
      { label: '', value: t => `<button type="button" data-remove-team="${t.team_id}">Remove</button>`, raw: true, left: true }], r.teams);
  }
  document.getElementById('teams').addEventListener('click', e => {
    const b = e.target.closest('button[data-remove-team]'); if (!b) return;
    const id = b.dataset.removeTeam;
    const rating = document.querySelector(`#teams input[data-rating="${id}"]`), bar = document.querySelector(`#teams input[data-bar="${id}"]`);
    if (rating) rating.value = EXPANSION_ELO;
    if (bar) bar.checked = true;
  });
  const moved = (a, b, digits) => { const d = Math.round((b - a) * 10 ** digits) / 10 ** digits; return d === 0 ? '' : `${d > 0 ? '+' : '−'}${Math.abs(d).toFixed(digits)}`; };
  const movedPct = (a, b) => { const d = Math.round(1000 * (b - a)) / 10; return d === 0 ? '' : `${d > 0 ? '+' : '−'}${Math.abs(d).toFixed(1)}`; };
  // A move that prints as zero is coloured as none.
  const cls = (a, b, digits = 1) => { const d = Math.round((b - a) * 10 ** digits) / 10 ** digits; return d > 0 ? 'good' : d < 0 ? 'bad' : 'muted'; };
  const clsPct = (a, b) => cls(100 * a, 100 * b);
  function renderResults(r) {
    const before = new Map(base.teams.map(t => [t.team_id, t]));
    const rows = r.teams.map(t => ({ ...t, before: before.get(t.team_id) })).filter(t => t.before);
    rows.sort((x, y) => y.p_champion - x.p_champion || y.expected_wins - x.expected_wins);
    const c = r.params || {};
    document.getElementById('note').textContent = `${r.information_cutoff_note} Both runs: ${r.iterations.toLocaleString()} simulated seasons from ${r.as_of}, seed ${r.seed}; qualification ${r.qualification}${c.first_round_games ? `, first round best of ${c.first_round_games}` : ''}. Changes: ${(c.rating_overrides || []).length} rating${(c.rating_overrides || []).length === 1 ? '' : 's'} set, ${(c.excluded_teams || []).length} team${(c.excluded_teams || []).length === 1 ? '' : 's'} barred, ${r.absences_applied} absence${r.absences_applied === 1 ? '' : 's'}, ${(c.worth_overrides || []).length} worth what-if${(c.worth_overrides || []).length === 1 ? '' : 's'}. A move within about one point is simulation noise.`;
    document.getElementById('results').innerHTML = table([
      { label: 'Team', value: t => `${t.code} · ${t.name}${t.excluded ? ' (barred)' : ''}`, left: true, cls: t => t.excluded ? 'muted' : '' },
      { label: 'Elo', value: t => `${num(t.before.elo_as_of, 0)} → ${num(t.elo_as_of, 0)}`, cls: t => cls(Math.round(t.before.elo_as_of), Math.round(t.elo_as_of), 0) },
      { label: 'Exp. wins', value: t => `${num(t.before.expected_wins, 1)} → ${num(t.expected_wins, 1)}`, left: true },
      { label: 'Moved', value: t => moved(t.before.expected_wins, t.expected_wins, 1), cls: t => cls(t.before.expected_wins, t.expected_wins) },
      { label: 'Playoffs', value: t => `${pct(t.before.p_playoffs)} → ${pct(t.p_playoffs)}`, left: true },
      { label: 'Moved (pts)', value: t => movedPct(t.before.p_playoffs, t.p_playoffs), cls: t => clsPct(t.before.p_playoffs, t.p_playoffs) },
      { label: 'Title', value: t => `${pct(t.before.p_champion, 1)} → ${pct(t.p_champion, 1)}`, left: true },
      { label: 'Moved (pts)', value: t => movedPct(t.before.p_champion, t.p_champion), cls: t => clsPct(t.before.p_champion, t.p_champion) },
      { label: 'Finals', value: t => `${pct(t.before.p_finals)} → ${pct(t.p_finals)}`, left: true }], rows);
    const pb = r.projected_bracket;
    document.getElementById('projected').innerHTML = pb ? bracketHtml(pb.bracket, pb.exact, pb.first_round) : '<p class="muted small">No simulated season produced a full bracket.</p>';
    document.getElementById('bracketNote').textContent = pb ? `The most likely seeding of the what-if, ${pb.exact_count.toLocaleString()} of ${pb.sampled.toLocaleString()} simulated seasons exactly.` : '';
  }
  loadBtn.addEventListener('click', async () => {
    loadBtn.disabled = true; runBtn.disabled = true; base = null;
    document.getElementById('teams').innerHTML = '';
    document.getElementById('results').innerHTML = ''; document.getElementById('note').textContent = '';
    document.getElementById('projected').innerHTML = ''; document.getElementById('bracketNote').textContent = '';
    try {
      const job = await runJob('/api/jobs/season', settings(), j => setStatus(status, `plain run ${j.status} ${pct(j.progress, 0)}`));
      base = job.result;
      renderTeams(base);
      setStatus(status, `plain run done: ${base.teams.length} teams from ${base.as_of}`);
    } catch (e) { setStatus(status, e.message, true); } finally { loadBtn.disabled = false; runBtn.disabled = !base; }
  });
  runBtn.addEventListener('click', async () => {
    if (!base) return;
    const p = { ...settings(), ...changes() };
    if (p.season !== base.season) { setStatus(status, 'The season changed: load it again first.', true); return; }
    runBtn.disabled = true; loadBtn.disabled = true;
    try {
      const job = await runJob('/api/jobs/season', p, j => setStatus(status, `what-if ${j.status} ${pct(j.progress, 0)}`));
      renderResults(job.result);
      setStatus(status, `done · ${job.model_version}`);
    } catch (e) { setStatus(status, e.message, true); } finally { runBtn.disabled = false; loadBtn.disabled = false; }
  });
})();
