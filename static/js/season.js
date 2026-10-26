(async () => {
  const { pct, num, esc, runJob, table, setStatus, chart, RESULT, getJSON, labelOf, bracketHtml } = NBA;
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
    const slot = (s, i) => `<div class="t"><span>${s.seed} ${esc(s.code)}</span><span>${num(s.elo, 0)} · ${pct(pb.exact[i][2], 1)}</span></div>`;
    const conf = (ci, name) => `<div class="conf"><h4>${name}</h4>` + pb.quarterfinals.filter(q => Math.floor(q.slot_a / 4) === ci).map(q => `<div class="series">${slot(pb.slots[q.slot_a], q.slot_a)}${slot(pb.slots[q.slot_b], q.slot_b)}<div class="round">${esc(pb.slots[q.slot_a].code)} wins quarterfinal ${pct(q.p_a)}</div></div>`).join('') + '</div>';
    document.getElementById('cupBracket').innerHTML = conf(0, 'East') + conf(1, 'West');
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
  showAdvantage();
  document.getElementById('run').addEventListener('click', async () => {
    const btn = document.getElementById('run'); btn.disabled = true;
    const pk = document.getElementById('pk').value; const radv = document.getElementById('radv').value;
    const p = { season: parseInt(document.getElementById('season').value), iterations: parseInt(document.getElementById('iterations').value), seed: parseInt(document.getElementById('seed').value) || 0,
      home_advantage: parseFloat(document.getElementById('adv').value), regular_home_advantage: radv === '' ? null : parseFloat(radv), regular_season_k: parseFloat(document.getElementById('k').value), playoff_k: pk === '' ? null : parseFloat(pk), playoff_scale: 1.0 };
    if (!document.getElementById('preseason').checked && document.getElementById('asOf').value) p.as_of = document.getElementById('asOf').value;
    if (document.getElementById('injuries').checked) p.use_injury_report = true;
    const name = document.getElementById('name').value.trim(); if (name) p.name = name;
    try {
      const job = await runJob('/api/jobs/season', p, j => setStatus(status, `${j.status} ${pct(j.progress, 0)}`));
      const r = job.result;
      document.getElementById('note').textContent = `${r.information_cutoff_note} Games used: ${r.games_played}; regular-season games simulated: ${r.games_remaining}; qualification format: ${r.qualification}. ${r.iterations.toLocaleString()} simulated seasons in ${r.elapsed_ms} ms.${provenance(p.season)} ${r.absences_applied ? ` ${r.absences_applied} absences from the injury report priced in; see the Players page.` : p.use_injury_report ? ' The injury report priced nothing: no report is stored for this season, or no listed player has a worth; see the Players page.' : ''}`;
      const cols = [
        { label: 'Team', value: t => `${t.code} · ${t.name}`, left: true }, { label: 'Conf', value: 'conference', left: true }, { label: 'Elo', value: t => num(t.elo_as_of, 0) },
        ...(r.absences_applied ? [{ label: 'Out (Elo)', value: t => t.absence_penalty ? `−${num(t.absence_penalty, 0)}` : '', cls: t => t.absence_penalty ? 'bad' : '', title: 'Mean rating penalty over the remaining games from the injury report' }] : []),
        { label: 'Record', value: t => `${t.wins}-${t.losses}` }, { label: 'Exp. wins', value: t => num(t.expected_wins, 1) },
        { label: 'Top 6', value: t => pct(t.p_top6) }, { label: 'Play-in', value: t => pct(t.p_play_in) }, { label: 'Playoffs', value: t => pct(t.p_playoffs) },
        { label: '2nd round', value: t => pct(t.p_second_round) }, { label: 'Conf. finals', value: t => pct(t.p_conf_finals) }, { label: 'Finals', value: t => pct(t.p_finals) },
        { label: 'Title', value: t => `${pct(t.p_champion, 2)} ± ${pct(1.96 * t.p_champion_se, 2)}`, cls: () => 'good' },
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
