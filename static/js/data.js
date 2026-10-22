(async () => {
  const { num, esc, getJSON, postJSON, table, setStatus } = NBA;
  async function loadHealth() {
    const h = await getJSON('/api/health'); const d = h.database;
    document.getElementById('health').innerHTML = `<div>Application version</div><div>${esc(h.app_version)} (up ${Math.round(h.uptime_seconds / 60)} min)</div><div>Database</div><div>${d.ok ? 'OK' : 'ERROR ' + esc(d.error)} · PostgreSQL ${esc(d.server_version)} · ${esc(d.database_size)} · ${num(d.latency_ms, 1)} ms</div><div>Connection</div><div>${esc(h.database_url)} · pool ${d.pool_size} (${d.pool_idle} idle)</div><div>Migrations applied</div><div>${d.migrations_applied}</div><div>Rows</div><div>${Object.entries(h.tables).map(([k, v]) => `${k}: ${v.toLocaleString()}`).join(' · ')}</div><div>Jobs</div><div>${Object.entries(h.jobs).map(([k, v]) => `${k}: ${v}`).join(' · ') || 'none'}</div><div>Simulation threads</div><div>${h.rayon_threads} · max iterations ${h.max_iterations.toLocaleString()}</div><div>Data directory</div><div>${esc(h.data_dir)}</div>`;
  }
  async function loadCoverage() {
    const c = await getJSON('/api/data/coverage');
    document.getElementById('sourceUrl').textContent = c.source_url;
    if (c.next_season && !document.getElementById('nextSeason').value) document.getElementById('nextSeason').value = c.next_season;
    const sync = document.getElementById('sync');
    if (c.results_source) {
      sync.disabled = false; sync.title = `${c.results_source.name} · ${c.results_source.base_url} · ${c.results_source.requests_per_minute} requests/min`;
      const s = c.results_sync;
      document.getElementById('syncHint').textContent = s
        ? `Daily sync of ${s.season - 1}-${String(s.season % 100).padStart(2, '0')} at ${s.time} ${s.zone}; next run ${new Date(s.next_run).toLocaleString()}. The button syncs the season entered above now (about three minutes on the free tier).`
        : 'Automatic daily sync is disabled (RESULTS_SYNC_ENABLED=false). The button syncs the season entered above now.';
    }
    else { sync.disabled = true; document.getElementById('syncHint').textContent = 'Not configured: set BALLDONTLIE_API_KEY in .env and restart the application to sync results from the BALLDONTLIE API.'; }
    document.getElementById('coverage').innerHTML = table([
      { label: 'Season', value: 'label', left: true }, { label: 'Teams', value: 'teams' }, { label: 'Games', value: 'games' }, { label: 'Play-in', value: 'play_in_games' }, { label: 'Playoff games', value: 'playoff_games' },
      { label: 'Regular season end', value: r => r.regular_season_end || '–', left: true }, { label: 'Playoffs', value: r => r.playoffs_start ? `${r.playoffs_start} → ${r.playoffs_end}` : '–', left: true },
      { label: 'Elo', value: r => r.games === 0 ? 'none' : (!r.computed_games ? 'imported' : (r.computed_games === r.games ? `computed (${r.computed_games})` : `imported + computed (${r.computed_games})`)), left: true, cls: r => r.games === 0 ? 'bad' : (r.computed_games ? 'warn' : '') },
      { label: 'Scheduled', value: r => r.scheduled_games || '' },
      { label: 'Complete', value: r => r.complete ? 'yes' : 'no', cls: r => r.complete ? 'good' : 'warn' }, { label: 'Champion', value: r => r.champion ? `${r.champion} (${r.champion_source})` : '–', left: true },
      { label: 'Bracket model', value: r => r.bracket_supported ? 'yes' : 'no (pre-2003)', left: true },
      { label: '', value: r => r.removable ? `<button class="remove" data-season="${r.season}" title="Delete this season's computed games and derived rows">Remove</button>` : '', raw: true }], c.seasons.slice().reverse())
      + (c.missing_seasons.length ? `<p class="warn">Missing seasons: ${c.missing_seasons.join(', ')}</p>` : '');
    document.querySelectorAll('#coverage button.remove').forEach(b => b.addEventListener('click', async () => {
      const season = parseInt(b.dataset.season); const label = `${season - 1}-${String(season % 100).padStart(2, '0')}`;
      if (!confirm(`Remove season ${label} (its computed games, standings and forecasts)? Imported archive data is never removed.`)) return;
      b.disabled = true;
      try { const r = await fetch(`/api/data/seasons/${season}`, { method: 'DELETE' }); const body = await r.json(); if (!r.ok) throw new Error(body.error || r.statusText); document.getElementById('importResult').textContent = JSON.stringify(body, null, 2); await refreshAll(); }
      catch (e) { document.getElementById('importResult').textContent = e.message; b.disabled = false; }
    }));
  }
  async function loadImports() {
    const list = await getJSON('/api/data/imports');
    document.getElementById('imports').innerHTML = table([
      { label: 'Started', value: r => new Date(r.started_at).toLocaleString(), left: true }, { label: 'Source', value: r => r.source + (r.source_url ? ' ' + r.source_url : ''), left: true }, { label: 'File', value: r => r.file_name || '', left: true },
      { label: 'Format', value: 'format', left: true }, { label: 'Status', value: 'status', left: true, cls: r => r.status === 'completed' ? 'good' : (r.status === 'failed' ? 'bad' : '') }, { label: 'Rows', value: r => r.rows_read.toLocaleString() },
      { label: 'Inserted', value: 'games_inserted' }, { label: 'Updated', value: 'games_updated' }, { label: 'Unchanged', value: 'games_unchanged' }, { label: 'Rejected', value: 'rows_rejected' },
      { label: 'Seasons', value: r => r.min_season ? `${r.min_season}–${r.max_season}` : '' }, { label: 'SHA-256', value: r => (r.file_sha256 || '').slice(0, 12), left: true }, { label: 'Notes', value: r => r.error || r.notes || '', left: true }], list);
  }
  async function refreshAll() { await Promise.all([loadHealth(), loadCoverage(), loadImports()]); }
  document.getElementById('refresh').addEventListener('click', async () => {
    const btn = document.getElementById('refresh'); btn.disabled = true; document.getElementById('importResult').textContent = 'Downloading and importing… this can take a minute.';
    try { const r = await postJSON('/api/data/refresh', { force: document.getElementById('force').checked }); document.getElementById('importResult').textContent = JSON.stringify(r, null, 2); await refreshAll(); }
    catch (e) { document.getElementById('importResult').textContent = e.message; } finally { btn.disabled = false; }
  });
  document.getElementById('uploadForm').addEventListener('submit', async e => {
    e.preventDefault(); const f = document.getElementById('file').files[0]; if (!f) return;
    const fd = new FormData(); fd.append('file', f); fd.append('force', document.getElementById('force').checked ? '1' : '0');
    document.getElementById('importResult').textContent = 'Uploading…';
    try { const r = await fetch('/api/data/upload', { method: 'POST', body: fd }); const body = await r.json(); document.getElementById('importResult').textContent = JSON.stringify(body, null, 2); await refreshAll(); }
    catch (err) { document.getElementById('importResult').textContent = err.message; }
  });
  document.getElementById('project').addEventListener('click', async () => {
    const season = parseInt(document.getElementById('nextSeason').value);
    const btn = document.getElementById('project'); btn.disabled = true; document.getElementById('importResult').textContent = `Projecting ${season - 1}-${String(season % 100).padStart(2, '0')}…`;
    try { const r = await postJSON('/api/data/project', { season }); document.getElementById('importResult').textContent = JSON.stringify(r, null, 2); await refreshAll(); }
    catch (e) { document.getElementById('importResult').textContent = e.message; } finally { btn.disabled = false; }
  });
  // Live sync status: the sync runs in the background; this polls /api/data/sync/status.
  const label = s => `${s - 1}-${String(s % 100).padStart(2, '0')}`;
  let lastShown = null, pollTimer = null;
  function describe(st) {
    const who = st.trigger === 'scheduled' ? 'Scheduled sync' : 'Sync';
    const secs = d => `${Math.round((Date.now() - new Date(d)) / 1000)} s`;
    switch (st.phase) {
      case 'fetching': {
        const wait = st.waiting_secs != null ? ` · ${st.rate_limited ? 'rate limited by the API, retrying' : 'next request'} in ${st.waiting_secs} s` : '';
        return `⏳ ${who} of ${label(st.season)} from BALLDONTLIE: ${st.requests} request${st.requests === 1 ? '' : 's'}, ${st.games.toLocaleString()} games so far${wait} · running ${secs(st.started_at)}`;
      }
      case 'done': return `✓ ${who} of ${label(st.season)} finished ${new Date(st.finished_at).toLocaleString()}: ${st.outcome.games_inserted} inserted, ${st.outcome.games_updated} updated, ${st.outcome.games_unchanged} unchanged.`;
      case 'failed': return `✗ ${who} of ${label(st.season)} failed ${new Date(st.finished_at).toLocaleString()}: ${st.error}`;
      default: return '';
    }
  }
  async function pollSync() {
    let st; try { st = await getJSON('/api/data/sync/status'); } catch (e) { return; }
    const el = document.getElementById('syncStatus'); el.textContent = describe(st);
    el.className = 'small ' + (st.phase === 'failed' ? 'bad' : st.phase === 'done' ? 'good' : '');
    const running = st.phase === 'fetching';
    document.getElementById('sync').disabled = running || !document.getElementById('sync').title;
    const key = `${st.phase}:${st.finished_at || ''}`;
    if (!running && lastShown && lastShown.startsWith('fetching') && key !== lastShown) {
      if (st.phase === 'done') document.getElementById('importResult').textContent = JSON.stringify(st.outcome, null, 2);
      await refreshAll();
    }
    lastShown = running ? 'fetching' : key;
    clearTimeout(pollTimer); if (running) pollTimer = setTimeout(pollSync, 1500);
  }
  document.getElementById('sync').addEventListener('click', async () => {
    const season = parseInt(document.getElementById('nextSeason').value);
    const btn = document.getElementById('sync'); btn.disabled = true;
    document.getElementById('importResult').textContent = '';
    try { await postJSON('/api/data/sync', { season }); lastShown = 'fetching'; await pollSync(); }
    catch (e) { document.getElementById('importResult').textContent = e.message; btn.disabled = false; }
  });
  document.getElementById('resultsForm').addEventListener('submit', async e => {
    e.preventDefault(); const f = document.getElementById('resultsFile').files[0]; if (!f) return;
    const fd = new FormData(); fd.append('season', document.getElementById('nextSeason').value); fd.append('file', f);
    document.getElementById('importResult').textContent = 'Uploading and computing Elo…';
    try { const r = await fetch('/api/data/results', { method: 'POST', body: fd }); const body = await r.json(); document.getElementById('importResult').textContent = JSON.stringify(body, null, 2); await refreshAll(); }
    catch (err) { document.getElementById('importResult').textContent = err.message; }
  });
  refreshAll().then(pollSync);
})();
