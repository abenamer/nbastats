(async () => {
  const { pct, esc, getJSON, postJSON, table } = NBA;
  async function load() {
    const kind = document.getElementById('kind').value;
    const jobs = await getJSON(`/api/jobs?limit=200${kind ? '&kind=' + kind : ''}`);
    document.getElementById('jobs').innerHTML = table([
      { label: 'Created', value: j => new Date(j.created_at).toLocaleString(), left: true }, { label: 'Kind', value: 'kind', left: true }, { label: 'Name', value: j => j.name || '', left: true },
      { label: 'Status', value: j => `<span class="status-${j.status}">${j.status}</span>${j.status === 'running' ? ` <progress value="${j.progress}" max="1"></progress>` : ''}`, raw: true, left: true },
      { label: 'Attempts', value: 'attempts' }, { label: 'Seed', value: j => j.seed ?? '' }, { label: 'Versions', value: j => `${j.app_version} / ${j.model_version}`, left: true },
      { label: 'Parameters', value: j => `<details><summary>show</summary><pre>${esc(JSON.stringify(j.params, null, 1))}</pre></details>`, raw: true, left: true }, { label: 'Error', value: j => j.error || '', left: true },
      { label: '', value: j => `${['failed', 'interrupted', 'cancelled'].includes(j.status) ? `<button data-retry="${j.id}">Retry</button>` : ''} ${['queued', 'running'].includes(j.status) ? `<button data-cancel="${j.id}">Cancel</button>` : ''} ${j.status !== 'running' ? `<button data-del="${j.id}">Delete</button>` : ''}`, raw: true }], jobs);
  }
  document.getElementById('jobs').addEventListener('click', async e => {
    const d = e.target.dataset;
    try {
      if (d.retry) await postJSON(`/api/jobs/${d.retry}/retry`);
      if (d.cancel) await postJSON(`/api/jobs/${d.cancel}/cancel`);
      if (d.del && confirm('Delete this job and its results?')) await fetch(`/api/jobs/${d.del}`, { method: 'DELETE' });
    } catch (err) { alert(err.message); }
    load();
  });
  document.getElementById('reload').addEventListener('click', load); document.getElementById('kind').addEventListener('change', load);
  load(); setInterval(load, 5000);
})();
