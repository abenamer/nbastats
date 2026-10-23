(async () => {
  const { pct, num, esc, getJSON, runJob, table, cards, setStatus, chart } = NBA;
  const status = document.getElementById('status');
  const editor = document.getElementById('alphaEditor');
  let names = null;
  function franchiseCount() { return Math.max(1, Math.min(200, parseInt(document.getElementById('n').value) || 30)); }
  function buildEditor(keepValues) {
    const n = franchiseCount();
    const old = keepValues ? currentAlphas() : [];
    editor.innerHTML = Array.from({ length: n }, (_, i) => {
      const label = names && names[i] ? names[i] : `Franchise ${i + 1}`;
      return `<label title="${esc(label)}"><span>${esc(label)}</span><input type="number" step="0.1" min="0" value="${old[i] ?? 1}" data-i="${i}"></label>`;
    }).join('');
    buildTarget();
  }
  function currentAlphas() { return [...editor.querySelectorAll('input')].map(i => parseFloat(i.value) || 0); }
  function currentNames() { return [...editor.querySelectorAll('label')].map(l => l.title); }
  // The franchise of interest follows the editor's names; a pick survives a rebuild when it still exists.
  const target = document.getElementById('target');
  function buildTarget(index = null) {
    const names = currentNames();
    const keep = Math.min(index ?? (parseInt(target.value) || 0), names.length - 1);
    target.innerHTML = names.map((n, i) => `<option value="${i}" ${i === keep ? 'selected' : ''}>${esc(n)}</option>`).join('');
  }
  function targetIndex() { return Math.min(Math.max(parseInt(target.value) || 0, 0), editor.querySelectorAll('input').length - 1); }
  // The iteration limit comes from the server, so the control and its tooltip show the real number.
  getJSON('/api/health').then(h => { if (h.max_iterations) { const row = document.getElementById('iterationsRow'); row.title += ` The limit is ${Number(h.max_iterations).toLocaleString()}.`; document.getElementById('iterations').max = h.max_iterations; } }).catch(() => {});
  document.getElementById('n').addEventListener('change', () => buildEditor(true));
  document.getElementById('useLeague').addEventListener('click', async () => {
    const fr = await getJSON('/api/franchises');
    names = fr.filter(f => f.active).map(f => f.key); document.getElementById('n').value = names.length; buildEditor(true);
  });
  document.getElementById('resetAlphas').addEventListener('click', () => { editor.querySelectorAll('input').forEach(i => i.value = 1); });
  document.getElementById('mode').addEventListener('change', e => document.getElementById('decayRow').classList.toggle('hidden', e.target.value !== 'decay'));
  async function updateExact() {
    const n = franchiseCount(), s = document.getElementById('seasons').value, k = document.getElementById('k').value;
    document.getElementById('exN').textContent = n; document.getElementById('exS').textContent = s; document.getElementById('exK').textContent = k;
    try { const d = await getJSON(`/api/exact/alpha?franchises=${n}&seasons=${s}&threshold=${k}`); document.getElementById('exact').textContent = pct(d.p_any_at_least_k, 2); } catch (e) { document.getElementById('exact').textContent = e.message; }
  }
  ['n', 'seasons', 'k'].forEach(id => document.getElementById(id).addEventListener('change', updateExact));
  function params() {
    const seasons = parseInt(document.getElementById('seasons').value), alphas = currentAlphas();
    const p = { franchises: currentNames(), alphas, seasons, iterations: parseInt(document.getElementById('iterations').value), threshold: parseInt(document.getElementById('k').value), seed: parseInt(document.getElementById('seed').value) || 0, target_index: targetIndex() };
    if (document.getElementById('mode').value === 'decay') {
      const hl = parseFloat(document.getElementById('halflife').value) || 5;
      p.schedule = Array.from({ length: seasons }, (_, s) => alphas.map(a => 1 + (a - 1) * Math.pow(0.5, s / hl)));
    }
    const name = document.getElementById('name').value.trim(); if (name) p.name = name;
    return p;
  }
  const targetOf = job => (job.result.franchises[(job.params && job.params.target_index) || 0] || job.result.franchises[0]);
  function render(job) {
    const r = job.result;
    const who = targetOf(job);
    cards(document.getElementById('cards'), [
      { label: `P(any franchise ≥ ${r.threshold})`, value: pct(r.p_any_at_least_k, 2), sub: `± ${pct(1.96 * r.p_any_at_least_k_se, 2)} (95% MC)${r.p_any_at_least_k_exact != null ? ` · exact ${pct(r.p_any_at_least_k_exact, 2)}` : ''}` },
      { label: `P(${who.name} ≥ ${r.threshold})`, value: pct(r.p_target_at_least_k, 2), sub: r.p_target_at_least_k_exact != null ? `exact ${pct(r.p_target_at_least_k_exact, 2)}` : 'season-dependent alphas: MC only' },
      { label: `E[# franchises ≥ ${r.threshold}]`, value: num(r.expected_count_at_least_k, 3) },
      { label: 'Top-5 share (mean)', value: pct(r.top5_share_mean) }, { label: 'Gini (mean)', value: num(r.gini_mean, 3) }, { label: 'HHI (mean)', value: num(r.hhi_mean, 3) },
      { label: 'Iterations', value: r.iterations.toLocaleString(), sub: `${r.elapsed_ms} ms · seed ${r.seed} · ${r.model_version}` }]);
    const labels = r.max_titles_distribution.map((_, i) => i);
    const ds = [{ label: 'Simulated', data: r.max_titles_distribution, backgroundColor: '#2563eb' }];
    if (r.max_titles_distribution_exact) ds.push({ label: 'Exact', data: r.max_titles_distribution_exact, backgroundColor: '#9ca3af' });
    chart('maxChart', { type: 'bar', data: { labels, datasets: ds }, options: { plugins: { title: { display: true, text: 'Maximum titles by any franchise' } }, scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
    chart('countChart', { type: 'bar', data: { labels: r.count_at_least_k_distribution.map((_, i) => i), datasets: [{ label: 'Probability', data: r.count_at_least_k_distribution, backgroundColor: '#16a34a' }] }, options: { plugins: { title: { display: true, text: `Number of franchises with ≥ ${r.threshold} titles` } }, scales: { y: { ticks: { callback: v => pct(v, 0) } } } } });
    const top = r.franchises.slice().sort((a, b) => b.expected_titles - a.expected_titles).slice(0, 30);
    chart('expectedChart', { type: 'bar', data: { labels: top.map(f => f.name), datasets: [{ label: 'Expected titles', data: top.map(f => f.expected_titles), backgroundColor: '#2563eb' }, { label: 'Simulated mean', data: top.map(f => f.simulated_mean_titles), backgroundColor: '#93c5fd' }] }, options: { scales: { y: { beginAtZero: true } } } });
    document.getElementById('results').innerHTML = `<div class="table-wrap">${table([
      { label: 'Franchise', value: 'name', left: true }, { label: 'Alpha', value: f => num(f.alpha, 2) }, { label: 'P(title)/season', value: f => pct(f.probability, 2) },
      { label: 'Expected titles', value: f => num(f.expected_titles, 3) }, { label: `P(≥ ${r.threshold})`, value: f => pct(f.p_at_least_k, 2) },
      { label: 'exact', value: f => f.p_at_least_k_exact == null ? '–' : pct(f.p_at_least_k_exact, 2) }], r.franchises.slice().sort((a, b) => b.expected_titles - a.expected_titles))}</div>`;
  }
  document.getElementById('run').addEventListener('click', async () => {
    const btn = document.getElementById('run'); btn.disabled = true;
    try { const job = await runJob('/api/jobs/alpha', params(), j => setStatus(status, `${j.status} ${pct(j.progress, 0)}`)); render(job); setStatus(status, `done in ${job.result.elapsed_ms} ms`); loadScenarios(); }
    catch (e) { setStatus(status, e.message, true); } finally { btn.disabled = false; }
  });
  async function loadScenarios() {
    const list = await getJSON('/api/scenarios/alpha');
    document.getElementById('scenarios').innerHTML = table([
      { label: '', value: j => `<input type="checkbox" data-id="${j.id}">`, raw: true, left: true }, { label: 'Name', value: 'name', left: true },
      { label: 'Created', value: j => new Date(j.created_at).toLocaleString(), left: true }, { label: 'Franchises', value: j => j.params.franchises.length }, { label: 'Seasons', value: j => j.params.seasons },
      { label: 'K', value: j => j.params.threshold }, { label: 'Iterations', value: j => j.params.iterations.toLocaleString() }, { label: 'Seed', value: j => j.params.seed }, { label: 'Mode', value: j => j.params.schedule ? 'season-dependent' : 'constant', left: true }, { label: 'App', value: 'app_version', left: true }, { label: 'Model', value: 'model_version', left: true },
      { label: '', value: j => `<button data-open="${j.id}">Open</button> <button data-del="${j.id}">Delete</button>`, raw: true }], list);
  }
  document.getElementById('scenarios').addEventListener('click', async e => {
    const open = e.target.dataset.open, del = e.target.dataset.del;
    if (open) { const j = await getJSON(`/api/jobs/${open}`); render(j); const p = j.params; document.getElementById('n').value = p.franchises.length; names = p.franchises; buildEditor(false); editor.querySelectorAll('input').forEach((i, k) => i.value = p.alphas[k]); buildTarget(p.target_index || 0); restoreMode(p); document.getElementById('seasons').value = p.seasons; document.getElementById('k').value = p.threshold; document.getElementById('iterations').value = p.iterations; document.getElementById('seed').value = p.seed; document.getElementById('name').value = j.name || ''; updateExact(); }
    if (del && confirm('Delete this scenario?')) { await fetch(`/api/jobs/${del}`, { method: 'DELETE' }); loadScenarios(); }
  });
  // The mode is restored from the stored table; the half-life is read back from the first step of
  // any alpha that is not 1 (every alpha at 1 leaves the current half-life, which changes nothing).
  function restoreMode(p) {
    const decay = !!p.schedule;
    document.getElementById('mode').value = decay ? 'decay' : 'constant';
    document.getElementById('decayRow').classList.toggle('hidden', !decay);
    if (!decay || p.schedule.length < 2) return;
    const i = p.alphas.findIndex(a => Math.abs(a - 1) > 1e-9);
    if (i < 0) return;
    const ratio = (p.schedule[1][i] - 1) / (p.alphas[i] - 1);
    if (ratio > 0 && ratio < 1) document.getElementById('halflife').value = (Math.log(0.5) / Math.log(ratio)).toFixed(1);
  }
  document.getElementById('refreshScenarios').addEventListener('click', loadScenarios);
  document.getElementById('compare').addEventListener('click', async () => {
    const ids = [...document.querySelectorAll('#scenarios input:checked')].map(i => i.dataset.id);
    const jobs = await Promise.all(ids.map(id => getJSON(`/api/jobs/${id}`)));
    document.getElementById('comparison').innerHTML = table([
      { label: 'Scenario', value: j => j.name || j.id.slice(0, 8), left: true }, { label: 'K', value: j => j.result.threshold }, { label: 'Seasons', value: j => j.result.seasons }, { label: 'Franchise', value: j => targetOf(j).name, left: true },
      { label: 'P(any ≥ K)', value: j => pct(j.result.p_any_at_least_k, 2) }, { label: 'exact', value: j => j.result.p_any_at_least_k_exact == null ? '–' : pct(j.result.p_any_at_least_k_exact, 2) },
      { label: 'P(target ≥ K)', value: j => pct(j.result.p_target_at_least_k, 2) }, { label: 'E[# ≥ K]', value: j => num(j.result.expected_count_at_least_k, 3) },
      { label: 'Top-5', value: j => pct(j.result.top5_share_mean) }, { label: 'Gini', value: j => num(j.result.gini_mean, 3) }, { label: 'HHI', value: j => num(j.result.hhi_mean, 3) }, { label: 'Version', value: j => `${j.app_version} / ${j.model_version}`, left: true }], jobs);
  });
  buildEditor(false); updateExact(); loadScenarios();
})();
