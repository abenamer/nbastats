(async () => {
  const { pct, num, esc, getJSON, runJob, table, setStatus, chart, color } = NBA;
  const status = document.getElementById('status');
  let result = null;
  function fill(sel, names) { sel.innerHTML = names.map(n => `<option value="${esc(n)}">${esc(n)}</option>`).join(''); }
  const ROUND = { first: 'First round', second: 'Conference semifinals', conf_finals: 'Conference finals', finals: 'Finals' };
  // Results stored before series scoring have no series summary, or an empty one.
  const series = m => (m.series_summary && m.series_summary.series) ? m.series_summary : null;
  const CRITERION = { series: 'series log loss per series', champion: 'champion log loss' };
  const signed = (v, d) => (v > 0 ? '+' : '') + num(v, d);
  function render(job) {
    result = job.result; const r = result;
    document.getElementById('summary').innerHTML = table([
      { label: 'Model', value: 'name', left: true }, { label: 'Seasons', value: m => m.summary.seasons },
      { label: 'Series', value: m => series(m) ? series(m).series : '–' },
      { label: 'Series Brier (± se)', value: m => series(m) ? `${num(series(m).mean_brier, 4)} ± ${num(series(m).brier_se, 4)}` : '–' },
      { label: 'Series log loss (± se)', value: m => series(m) ? `${num(series(m).mean_log_loss, 4)} ± ${num(series(m).log_loss_se, 4)}` : '–' },
      { label: 'Series skill vs equal', value: m => series(m) ? pct(series(m).brier_skill_vs_equal) : '–' },
      { label: 'Δ log loss vs first (± se)', value: m => m.series_vs_first ? `${signed(m.series_vs_first.log_loss, 4)} ± ${num(m.series_vs_first.log_loss_se, 4)}` : '–', cls: m => (m.series_vs_first && Math.abs(m.series_vs_first.log_loss) > 2 * m.series_vs_first.log_loss_se) ? (m.series_vs_first.log_loss < 0 ? 'good' : 'bad') : '' },
      { label: 'Series favourite won', value: m => series(m) ? (series(m).with_favourite ? `${pct(series(m).favourite_won)} of ${series(m).with_favourite} (mean P ${pct(series(m).mean_p_favourite)})` : 'no favourite') : '–' },
      { label: 'Champion Brier (± se)', value: m => `${num(m.summary.mean_brier, 3)} ± ${num(m.summary.brier_se, 3)}` }, { label: 'Champion skill vs equal', value: m => pct(m.summary.brier_skill_vs_equal) },
      { label: 'Champion log loss (± se)', value: m => `${num(m.summary.mean_log_loss, 2)} ± ${num(m.summary.log_loss_se, 2)}` },
      { label: 'Champion favourite won', value: m => pct(m.summary.top_pick_accuracy) }, { label: 'Mean P(champion)', value: m => pct(m.summary.mean_p_winner) },
      { label: 'Parameters', value: m => m.spec ? JSON.stringify(m.spec) : (m.chosen_params ? 'rolling: ' + m.chosen_params.map(c => `${c[0]}→H${c[1]}/s${c[2]}`).join(' ') : ''), left: true }], r.models)
      + `<p class="muted small">Seasons evaluated: ${r.seasons_evaluated.map(NBA.labelOf).join(', ')}${r.seasons_skipped.length ? '. Skipped: ' + r.seasons_skipped.map(s => `${NBA.labelOf(s[0])} (${esc(s[1])})`).join('; ') : ''}.${r.in_sample_best ? ` In-sample best grid point (not a fair score): home advantage ${r.in_sample_best[0]}, scale ${r.in_sample_best[1]}, ${CRITERION[r.tuning_criterion] || 'log loss'} ${num(r.in_sample_best[2], 3)}.` : ''} Job ${job.id.slice(0, 8)} · ${job.app_version} / ${job.model_version}.</p>`;
    fill(document.getElementById('calModel'), r.models.map(m => m.name)); fill(document.getElementById('seasonModel'), r.models.map(m => m.name)); fill(document.getElementById('roundModel'), r.models.map(m => m.name));
    drawCalibration(); drawSeasons(); drawRounds();
    const elo = r.models[0];
    chart('winnerChart', { type: 'bar', data: { labels: elo.seasons.map(s => NBA.labelOf(s.season)), datasets: r.models.map((m, i) => ({ label: m.name, data: elo.seasons.map(s => (m.seasons.find(x => x.season === s.season) || {}).p_champion ?? null), backgroundColor: color(i) })) }, options: { scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
  }
  function drawCalibration() {
    if (!result) return;
    const m = result.models.find(x => x.name === document.getElementById('calModel').value) || result.models[0];
    const bins = (m.calibration[document.getElementById('stage').value] || []).filter(b => b.count > 0);
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
      { label: 'Champion Brier', value: s => num(s.brier, 4) }, { label: 'Champion log loss', value: s => num(s.log_loss, 3) },
      { label: 'Series favourites won', value: s => !s.series ? '–' : s.series_with_favourite ? `${s.series_hits} of ${s.series_with_favourite}` : 'no favourite' }, { label: 'Series Brier', value: s => s.series ? num(s.series_brier, 4) : '–' }, { label: 'Series log loss', value: s => s.series ? num(s.series_log_loss, 4) : '–' }], m.seasons);
  }
  function drawRounds() {
    if (!result) return;
    const m = result.models.find(x => x.name === document.getElementById('roundModel').value) || result.models[0];
    const rows = (series(m) ? series(m).by_round : []).filter(r => r.series);
    document.getElementById('rounds').innerHTML = rows.length ? table([
      { label: 'Round', value: r => ROUND[r.round] || r.round, left: true }, { label: 'Series', value: 'series' },
      { label: 'Brier', value: r => num(r.mean_brier, 4) }, { label: 'Log loss', value: r => num(r.mean_log_loss, 4) },
      { label: 'Favourite won', value: r => r.with_favourite ? `${pct(r.favourite_won)} of ${r.with_favourite}` : 'no favourite' }, { label: 'Mean P(favourite)', value: r => r.with_favourite ? pct(r.mean_p_favourite) : '–' }], rows) : '<p class="muted small">This backtest was run before series scoring; run it again.</p>';
    // Colours follow the model's place in the result, as in the other charts.
    const withSeries = result.models.map((x, i) => [x, i]).filter(([x]) => series(x));
    const labels = withSeries.length ? series(withSeries[0][0]).by_round.map(r => ROUND[r.round] || r.round) : [];
    chart('roundChart', { type: 'bar', data: { labels, datasets: withSeries.map(([x, i]) => ({ label: x.name, data: series(x).by_round.map(r => r.with_favourite ? r.favourite_won : null), backgroundColor: color(i) })) },
      options: { maintainAspectRatio: false, plugins: { title: { display: true, text: 'Share of series the favourite won' } }, scales: { y: { min: 0, max: 1, ticks: { callback: v => pct(v, 0) } } } } });
  }
  document.getElementById('stage').addEventListener('change', drawCalibration); document.getElementById('calModel').addEventListener('change', drawCalibration); document.getElementById('seasonModel').addEventListener('change', drawSeasons); document.getElementById('roundModel').addEventListener('change', drawRounds);
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
