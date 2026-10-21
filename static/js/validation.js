(async () => {
  const { pct, num, esc, getJSON, runJob, table, setStatus, chart, color } = NBA;
  const status = document.getElementById('status');
  let result = null;
  function fill(sel, names) { sel.innerHTML = names.map(n => `<option value="${esc(n)}">${esc(n)}</option>`).join(''); }
  function render(job) {
    result = job.result; const r = result;
    document.getElementById('summary').innerHTML = table([
      { label: 'Model', value: 'name', left: true }, { label: 'Seasons', value: m => m.summary.seasons }, { label: 'Brier (mean ± se)', value: m => `${num(m.summary.mean_brier, 4)} ± ${num(m.summary.brier_se, 4)}` },
      { label: 'Skill vs equal', value: m => pct(m.summary.brier_skill_vs_equal) }, { label: 'Log loss (mean ± se)', value: m => `${num(m.summary.mean_log_loss, 3)} ± ${num(m.summary.log_loss_se, 3)}` },
      { label: 'Favourite won', value: m => pct(m.summary.top_pick_accuracy) }, { label: 'Mean P(winner)', value: m => pct(m.summary.mean_p_winner) },
      { label: 'Parameters', value: m => m.spec ? JSON.stringify(m.spec) : (m.chosen_params ? 'rolling: ' + m.chosen_params.map(c => `${c[0]}→H${c[1]}/s${c[2]}`).join(' ') : ''), left: true }], r.models)
      + `<p class="muted small">Seasons evaluated: ${r.seasons_evaluated.map(NBA.labelOf).join(', ')}${r.seasons_skipped.length ? '. Skipped: ' + r.seasons_skipped.map(s => `${NBA.labelOf(s[0])} (${esc(s[1])})`).join('; ') : ''}.${r.in_sample_best ? ` In-sample best grid point (not a fair score): home advantage ${r.in_sample_best[0]}, scale ${r.in_sample_best[1]}, log loss ${num(r.in_sample_best[2], 3)}.` : ''} Job ${job.id.slice(0, 8)} · ${job.app_version} / ${job.model_version}.</p>`;
    fill(document.getElementById('calModel'), r.models.map(m => m.name)); fill(document.getElementById('seasonModel'), r.models.map(m => m.name));
    drawCalibration(); drawSeasons();
    const elo = r.models[0];
    chart('winnerChart', { type: 'bar', data: { labels: elo.seasons.map(s => NBA.labelOf(s.season)), datasets: r.models.map((m, i) => ({ label: m.name, data: elo.seasons.map(s => (m.seasons.find(x => x.season === s.season) || {}).p_champion ?? null), backgroundColor: color(i) })) }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
  }
  function drawCalibration() {
    if (!result) return;
    const m = result.models.find(x => x.name === document.getElementById('calModel').value) || result.models[0];
    const bins = m.calibration[document.getElementById('stage').value].filter(b => b.count > 0);
    chart('calChart', { data: { labels: bins.map(b => `${pct(b.lower, 0)}–${pct(b.upper, 0)} (n=${b.count})`), datasets: [
      { type: 'bar', label: 'Observed frequency', data: bins.map(b => b.observed), backgroundColor: '#2563eb' },
      { type: 'line', label: 'Mean predicted', data: bins.map(b => b.mean_predicted), borderColor: '#111827', pointRadius: 3 },
      { type: 'line', label: '95% CI low', data: bins.map(b => b.observed_ci95[0]), borderColor: '#9ca3af', borderDash: [4, 4], pointRadius: 0 },
      { type: 'line', label: '95% CI high', data: bins.map(b => b.observed_ci95[1]), borderColor: '#9ca3af', borderDash: [4, 4], pointRadius: 0 }] },
      options: { scales: { y: { min: 0, max: 1, ticks: { callback: v => pct(v, 0) } } } } });
  }
  function drawSeasons() {
    if (!result) return;
    const m = result.models.find(x => x.name === document.getElementById('seasonModel').value) || result.models[0];
    document.getElementById('seasons').innerHTML = table([
      { label: 'Season', value: s => NBA.labelOf(s.season), left: true }, { label: 'Champion', value: 'champion', left: true }, { label: 'P(champion)', value: s => pct(s.p_champion) },
      { label: 'Favourite', value: 'favorite', left: true }, { label: 'P(favourite)', value: s => pct(s.favorite_prob) }, { label: 'Hit', value: s => s.hit ? '✓' : '', cls: s => s.hit ? 'good' : '' },
      { label: 'Brier', value: s => num(s.brier, 4) }, { label: 'Log loss', value: s => num(s.log_loss, 3) }], m.seasons);
  }
  document.getElementById('stage').addEventListener('change', drawCalibration); document.getElementById('calModel').addEventListener('change', drawCalibration); document.getElementById('seasonModel').addEventListener('change', drawSeasons);
  document.getElementById('run').addEventListener('click', async () => {
    const btn = document.getElementById('run'); btn.disabled = true;
    const p = { from_season: parseInt(document.getElementById('from').value), to_season: parseInt(document.getElementById('to').value),
      models: [{ model: 'elo', home_advantage: parseFloat(document.getElementById('adv').value), scale: parseFloat(document.getElementById('scale').value), k: null }, { model: 'equal' }, { model: 'win_pct' }, { model: 'seed', p_better: 0.75 }],
      tuning: document.getElementById('tuning').checked ? { home_advantages: [0, 50, 100, 150], scales: [0.75, 1.0, 1.25, 1.5, 1.75], min_train_seasons: 5 } : null };
    try { const job = await runJob('/api/jobs/backtest', p, j => setStatus(status, `${j.status} ${pct(j.progress, 0)}`)); render(job); setStatus(status, `done in ${job.result.elapsed_ms} ms`); }
    catch (e) { setStatus(status, e.message, true); } finally { btn.disabled = false; }
  });
  document.getElementById('loadLatest').addEventListener('click', async () => { try { render(await getJSON('/api/backtests/latest')); setStatus(status, 'loaded latest backtest'); } catch (e) { setStatus(status, e.message, true); } });
  try { render(await getJSON('/api/backtests/latest')); } catch (e) { setStatus(status, 'No backtest yet: click "Run backtest".'); }
})();
