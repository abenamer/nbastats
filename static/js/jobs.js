(async () => {
  const { pct, esc, getJSON, postJSON, table } = NBA;
  // The list as last drawn: a poll that brings the same list redraws nothing, so an open
  // Parameters toggle and a sort order survive it.
  let drawn = '';
  async function load(force = false) {
    const kind = document.getElementById('kind').value;
    const jobs = await getJSON(`/api/jobs?limit=200${kind ? '&kind=' + kind : ''}`);
    const key = `${kind}|${JSON.stringify(jobs)}`;
    if (!force && key === drawn) return;
    drawn = key;
    const host = document.getElementById('jobs');
    // The table is rebuilt whole, so the toggles that were open are opened again after.
    const open = new Set([...host.querySelectorAll('details[open][data-job]')].map(d => d.dataset.job));
    host.innerHTML = table([
      { label: 'Created', value: j => new Date(j.created_at).toLocaleString(), left: true }, { label: 'Kind', value: 'kind', left: true }, { label: 'Name', value: j => j.name || '', left: true },
      { label: 'Status', value: j => `<span class="status-${esc(j.status)}">${esc(j.status)}</span>${j.status === 'running' ? ` <progress value="${j.progress}" max="1"></progress>` : ''}`, raw: true, left: true },
      { label: 'Attempts', value: 'attempts' }, { label: 'Seed', value: j => j.seed ?? '' }, { label: 'Versions', value: j => `${j.app_version} / ${j.model_version}`, left: true },
      { label: 'Parameters', value: j => `<details data-job="${esc(j.id)}"><summary>show</summary><pre>${esc(JSON.stringify(j.params, null, 1))}</pre></details>`, raw: true, left: true }, { label: 'Error', value: j => j.error || '', left: true },
      { label: '', value: j => `${['failed', 'interrupted', 'cancelled'].includes(j.status) ? `<button data-retry="${j.id}">Retry</button>` : ''} ${['queued', 'running'].includes(j.status) ? `<button data-cancel="${j.id}">Cancel</button>` : ''} ${j.status !== 'running' ? `<button data-del="${j.id}">Delete</button>` : ''}`, raw: true }], jobs);
    for (const d of host.querySelectorAll('details[data-job]')) if (open.has(d.dataset.job)) d.open = true;
  }
  document.getElementById('jobs').addEventListener('click', async e => {
    const d = e.target.dataset;
    // Only the three buttons change anything; a click on a cell or on the Parameters toggle
    // must not redraw the table, or the toggle closes as soon as it opens.
    if (!d.retry && !d.cancel && !d.del) return;
    try {
      if (d.retry) await postJSON(`/api/jobs/${d.retry}/retry`);
      if (d.cancel) await postJSON(`/api/jobs/${d.cancel}/cancel`);
      if (d.del && confirm('Delete this job and its results?')) await fetch(`/api/jobs/${d.del}`, { method: 'DELETE' });
    } catch (err) { alert(err.message); }
    load(true);
  });
  document.getElementById('reload').addEventListener('click', () => load(true)); document.getElementById('kind').addEventListener('change', () => load(true));
  load(true); setInterval(load, 5000);
})();
