(async () => {
  const { num, esc, getJSON, postJSON, table, setStatus } = NBA;
  const label = s => `${s - 1}-${String(s % 100).padStart(2, '0')}`;
  let currentSeason = null, configured = false, updating = false;
  async function loadHealth() {
    const h = await getJSON('/api/health'); const d = h.database;
    document.getElementById('health').innerHTML = `<div>Application version</div><div>${esc(h.app_version)} (up ${Math.round(h.uptime_seconds / 60)} min)</div><div>Database</div><div>${d.ok ? 'OK' : 'ERROR ' + esc(d.error)} · PostgreSQL ${esc(d.server_version)} · ${esc(d.database_size)} · ${num(d.latency_ms, 1)} ms</div><div>Connection</div><div>${esc(h.database_url)} · pool ${d.pool_size} (${d.pool_idle} idle)</div><div>Migrations applied</div><div>${d.migrations_applied}</div><div>Rows</div><div>${Object.entries(h.tables).map(([k, v]) => `${k}: ${v.toLocaleString()}`).join(' · ')}</div><div>Jobs</div><div>${Object.entries(h.jobs).map(([k, v]) => `${k}: ${v}`).join(' · ') || 'none'}</div><div>Simulation threads</div><div>${h.rayon_threads} · max iterations ${h.max_iterations.toLocaleString()}</div><div>Data directory</div><div>${esc(h.data_dir)}</div>`;
  }
  async function loadCoverage() {
    const c = await getJSON('/api/data/coverage');
    document.getElementById('sourceUrl').textContent = c.source_url;
    if (c.next_season && !document.getElementById('nextSeason').value) document.getElementById('nextSeason').value = c.next_season;
    if (!document.getElementById('playerSeason').value) document.getElementById('playerSeason').value = c.current_season;
    if (!document.getElementById('pastSeason').value) document.getElementById('pastSeason').value = c.current_season - 1;
    document.getElementById('syncPast').disabled = !c.results_source;
    currentSeason = c.current_season; configured = !!c.results_source;
    const btn = document.getElementById('update');
    btn.title = c.results_source ? `${c.results_source.name} · ${c.results_source.base_url} · ${c.results_source.requests_per_minute} requests/min` : '';
    if (!updating) btn.disabled = !configured;
    const s = c.results_sync;
    document.getElementById('updateHint').textContent = !configured
      ? 'Not configured: set BALLDONTLIE_API_KEY in .env and restart the application.'
      : `Updates ${label(c.current_season)}.${s ? ` The application also runs the update every day at ${s.time} ${s.zone}; next run ${new Date(s.next_run).toLocaleString()}.` : ' The daily update is off (RESULTS_SYNC_ENABLED=false).'} A first run of a season fetches its whole schedule; later runs take a few requests.`;
    document.getElementById('coverage').innerHTML = table([
      { label: 'Season', value: 'label', left: true }, { label: 'Teams', value: 'teams' }, { label: 'Games', value: 'games' }, { label: 'Play-in', value: 'play_in_games' }, { label: 'Playoff games', value: 'playoff_games' },
      { label: 'Regular season end', value: r => r.regular_season_end || '–', left: true }, { label: 'Playoffs', value: r => r.playoffs_start ? `${r.playoffs_start} → ${r.playoffs_end}` : '–', left: true },
      { label: 'Results from', value: r => r.games === 0 ? 'none' : (r.archive_games === r.games ? 'archive' : (r.archive_games ? `archive (${r.archive_games}) + results (${r.games - r.archive_games})` : 'results or projection')), left: true, cls: r => r.games === 0 ? 'bad' : (r.archive_games === r.games ? '' : 'warn') },
      { label: 'Scheduled', value: r => r.scheduled_games || '' },
      { label: 'Complete', value: r => r.complete ? 'yes' : 'no', cls: r => r.complete ? 'good' : 'warn' }, { label: 'Champion', value: r => r.champion ? `${r.champion} (${r.champion_source})` : '–', left: true },
      { label: 'Bracket model', value: r => r.bracket_supported ? 'yes' : 'no (pre-2003)', left: true },
      { label: 'Actions', value: r => r.removable ? `<button class="remove compact" data-season="${r.season}" title="Delete this season's games and derived rows (no archive games)">Remove</button>` : '', raw: true, left: true }], c.seasons.slice().reverse())
      + (c.missing_seasons.length ? `<p class="warn">Missing seasons: ${c.missing_seasons.join(', ')}</p>` : '');
    document.querySelectorAll('#coverage button.remove').forEach(b => b.addEventListener('click', async () => {
      const season = parseInt(b.dataset.season); const label = `${season - 1}-${String(season % 100).padStart(2, '0')}`;
      if (!confirm(`Remove season ${label} (its games, standings and forecasts)? Seasons with archive games are never removed.`)) return;
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
  async function refreshAll() { await loadCoverage(); await Promise.all([loadHealth(), loadImports(), loadPlayers()]); }
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
    let st;
    try { st = await getJSON('/api/data/sync/status'); }
    catch (e) { clearTimeout(pollTimer); if (lastShown === 'fetching' && !updating) pollTimer = setTimeout(pollSync, 3000); return; }
    const el = document.getElementById('syncStatus'); el.textContent = describe(st);
    el.className = 'small ' + (st.phase === 'failed' ? 'bad' : st.phase === 'done' ? 'good' : 'muted');
    const running = st.phase === 'fetching';
    const key = `${st.phase}:${st.finished_at || ''}`;
    if (!running && !updating && lastShown && lastShown.startsWith('fetching') && key !== lastShown) await refreshAll();
    lastShown = running ? 'fetching' : key;
    // During an update its own poll drives this one.
    clearTimeout(pollTimer); if (running && !updating) pollTimer = setTimeout(pollSync, 1500);
  }
  // Update Data: one run over the current season; this polls /api/data/update/status and,
  // while it runs, the two steps' own status lines.
  let updateTimer = null, lastUpdate = null, lastUpdateStep = null, lastPlayerPhase = null, planNote = false;
  // What holds the player lock while the update waits for it, and when it lets go.
  function holder() {
    const p = lastPlayerPhase;
    if (!p || p.phase !== 'fetching') return 'another player sync to end';
    const who = (p.trigger === 'scheduled' ? 'the scheduled ' : 'the ') + playerKind(p.kind).replace(/^./, c => c.toLowerCase());
    if (p.kind && p.kind.kind === 'backfill' && p.trigger === 'scheduled') return `${who} to reach the end of its season`;
    if (p.kind && p.kind.kind === 'backfill_all') return `${who} to end (it holds the player data until then)`;
    return `${who} to end`;
  }
  function describeUpdate(st) {
    const secs = d => `${Math.round((Date.now() - new Date(d)) / 1000)} s`;
    switch (st.phase) {
      case 'running': {
        const step = { results: 'step 1 of 2, results and schedule', waiting_for_players: `step 2 of 2, waiting for ${holder()}`, players: 'step 2 of 2, box scores, injury report and rosters' }[st.step] || st.step;
        return `⏳ Updating ${label(st.season)}: ${step} · running ${secs(st.started_at)}${st.results_error ? ` · results: ${st.results_error}` : ''}`;
      }
      case 'done': {
        const problems = [st.results_error && `results: ${st.results_error}`, st.players_error && `box scores, injury report and rosters: ${st.players_error}`].filter(Boolean);
        const note = st.players_note ? ` Note: ${st.players_note}.` : '';
        return problems.length
          ? `✗ Update of ${label(st.season)} finished ${new Date(st.finished_at).toLocaleString()} with problems. ${problems.join(' · ')}${note}`
          : `✓ ${label(st.season)} up to date as of ${new Date(st.finished_at).toLocaleString()}.${note}`;
      }
      default: return '';
    }
  }
  async function pollUpdate() {
    let st;
    try { st = await getJSON('/api/data/update/status'); }
    catch (e) { clearTimeout(updateTimer); if (updating) updateTimer = setTimeout(pollUpdate, 3000); return; }
    const el = document.getElementById('updateStatus');
    updating = st.phase === 'running';
    document.getElementById('update').disabled = updating || !configured;
    if (updating) { await Promise.all([pollSync(), pollPlayers()]); }
    el.textContent = describeUpdate(st);
    el.className = 'small ' + (st.phase === 'done' ? (st.results_error || st.players_error ? 'bad' : 'good') : st.results_error ? 'bad' : '');
    // The results are stored once the first step ends: show them without waiting for the rest.
    if (updating && lastUpdateStep === 'results' && st.step !== 'results') await refreshAll();
    lastUpdateStep = updating ? st.step : null;
    const key = `${st.phase}:${st.finished_at || ''}`;
    if (st.phase === 'idle' && lastUpdate === 'running') {
      // The server restarted under the update: its phase is gone with it.
      el.textContent = '✗ The update did not finish: the server restarted. Click Update Data again.'; el.className = 'small bad';
    }
    // The run just ended: one last look at each step's line, then the tables.
    if (!updating && lastUpdate === 'running' && key !== lastUpdate) {
      await Promise.all([pollSync(), pollPlayers()]);
      await refreshAll();
    }
    // A plan without player data is a note; the daily player line it failed need not shout.
    planNote = st.phase === 'done' && !!st.players_note;
    if (planNote && lastPlayerPhase && lastPlayerPhase.phase === 'failed') document.getElementById('playerStatus').className = 'small muted';
    lastUpdate = updating ? 'running' : key;
    clearTimeout(updateTimer); if (updating) updateTimer = setTimeout(pollUpdate, 1500);
  }
  document.getElementById('update').addEventListener('click', async () => {
    const btn = document.getElementById('update'); btn.disabled = true;
    const el = document.getElementById('updateStatus');
    try { await postJSON('/api/data/update', {}); lastUpdate = 'running'; await pollUpdate(); }
    catch (e) {
      el.textContent = e.message; el.className = 'small bad'; btn.disabled = !configured;
      // Something else holds a lock: follow it, an update started elsewhere included.
      await Promise.all([pollSync(), pollPlayers()]);
      if (/update is already running/.test(e.message)) { lastUpdate = 'running'; await pollUpdate(); }
    }
  });
  document.getElementById('syncPast').addEventListener('click', async () => {
    const season = parseInt(document.getElementById('pastSeason').value);
    const el = document.getElementById('syncStatus');
    if (!season) { el.textContent = 'Enter the season to sync.'; el.className = 'small bad'; return; }
    if (season >= currentSeason) { el.textContent = `${label(season)} is the current season or later: Update Data covers the current season.`; el.className = 'small bad'; return; }
    try { await postJSON('/api/data/sync', { season }); el.textContent = `Syncing ${label(season)}…`; el.className = 'small'; lastShown = 'fetching'; await pollSync(); }
    catch (e) { el.textContent = e.message; el.className = 'small bad'; }
  });
  document.getElementById('resultsForm').addEventListener('submit', async e => {
    e.preventDefault(); const f = document.getElementById('resultsFile').files[0]; if (!f) return;
    const fd = new FormData(); fd.append('season', document.getElementById('nextSeason').value); fd.append('file', f);
    document.getElementById('importResult').textContent = 'Uploading and computing Elo…';
    try { const r = await fetch('/api/data/results', { method: 'POST', body: fd }); const body = await r.json(); document.getElementById('importResult').textContent = JSON.stringify(body, null, 2); await refreshAll(); }
    catch (err) { document.getElementById('importResult').textContent = err.message; }
  });
  // Players, box scores and injury reports: coverage, backfill and the live run status.
  let lastPlayerShown = null, playerTimer = null;
  function playerKind(k) {
    if (!k) return 'Player sync';
    if (k.kind === 'backfill') return `Box-score backfill of ${label(k.season)}`;
    if (k.kind === 'backfill_all') return `Box-score backfill of every season ${label(k.from)} to ${label(k.to)}`;
    if (k.kind === 'daily') return `Daily player sync of ${label(k.season)}`;
    if (k.kind === 'all_players') return 'Player list load';
    if (k.kind === 'active_players') return 'Roster refresh';
    return 'Injury report fetch';
  }
  function describePlayers(st) {
    const who = (st.trigger === 'scheduled' ? 'Scheduled ' : '') + playerKind(st.kind);
    const secs = d => `${Math.round((Date.now() - new Date(d)) / 1000)} s`;
    const outcome = o => `${o.dates_fetched} date${o.dates_fetched === 1 ? '' : 's'}, ${o.lines.toLocaleString()} lines (${o.matched.toLocaleString()} matched, ${o.unmatched} unmatched), ${o.players_upserted} players${o.injury_report ? `; injury report of ${o.injury_report[0]} with ${o.injury_report[1]} lines` : ''}${o.players_loaded != null ? `; ${o.players_loaded.toLocaleString()} players loaded` : ''}${o.active_players != null ? `; ${o.active_players} players on rosters` : ''}`;
    switch (st.phase) {
      case 'fetching': {
        const wait = st.waiting_secs != null ? ` · ${st.rate_limited ? 'rate limited by the API, retrying' : 'next request'} in ${st.waiting_secs} s` : '';
        const season = st.season ? ` ${label(st.season)} (season ${st.seasons_done + 1} of ${st.seasons_total}),` : '';
        const dates = st.dates_total ? ` date ${st.dates_done + 1} of ${st.dates_total}${st.current_date ? ` (${st.current_date})` : ''},` : '';
        return `⏳ ${who}:${season}${dates} ${st.requests} request${st.requests === 1 ? '' : 's'}, ${st.lines.toLocaleString()} lines so far${wait} · running ${secs(st.started_at)}`;
      }
      case 'done': return `✓ ${who} finished ${new Date(st.finished_at).toLocaleString()}: ${outcome(st.outcome)}.`;
      case 'failed': return `✗ ${who} failed ${new Date(st.finished_at).toLocaleString()}: ${st.error}${st.outcome.dates_fetched ? ` Stored before the failure: ${outcome(st.outcome)}.` : ''}`;
      default: return '';
    }
  }
  async function loadPlayers() {
    const season = document.getElementById('playerSeason').value;
    const d = await getJSON('/api/data/players' + (season ? `?season=${encodeURIComponent(season)}` : ''));
    const c = d.coverage;
    document.getElementById('playerCoverage').innerHTML = `<div>Players</div><div>${c.players.toLocaleString()} (${c.active_players} on current rosters)</div><div>Box-score lines</div><div>${c.appearances.toLocaleString()}</div><div>Dates fetched</div><div>${c.dates_fetched.toLocaleString()}${c.first_date_fetched ? ` (${c.first_date_fetched} → ${c.last_date_fetched})` : ''}</div><div>Injury reports</div><div>${c.injury_report_dates} day${c.injury_report_dates === 1 ? '' : 's'}${c.last_injury_report ? `, latest ${c.last_injury_report}` : ''}</div>` + (d.season ? `<div>${label(d.season)}</div><div>${d.fetches.length} date${d.fetches.length === 1 ? '' : 's'} fetched, ${d.dates_missing} with played games and no box scores yet</div>` : '');
    const configured = d.configured;
    ['playerBackfillAll', 'playerLoadAll'].forEach(id => { document.getElementById(id).disabled = !configured; });
    const left = d.backfill.filter(b => b.dates_fetched < b.dates_total);
    const datesLeft = left.reduce((n, b) => n + b.dates_total - b.dates_fetched, 0);
    const loop = d.backfill_from ? ` The background backfill from ${label(d.backfill_from)} is on (PLAYER_BACKFILL_FROM) and runs season by season until nothing is left.` : (configured ? ' Set PLAYER_BACKFILL_FROM to a season end year to let the application backfill every season on its own, across restarts.' : '');
    document.getElementById('playerBackfillSummary').textContent = d.backfill.length
      ? (left.length ? `Box scores still missing in ${left.length} of ${d.backfill.length} seasons (${datesLeft.toLocaleString()} dates): ${left.slice(-6).map(b => `${label(b.season)} ${b.dates_fetched}/${b.dates_total}`).join(', ')}${left.length > 6 ? ', …' : ''}.` : `Every played date of all ${d.backfill.length} seasons has its box scores.`) + loop
      : '';
    document.getElementById('playerHint').textContent = configured
      ? 'Backfill every season fetches the box scores of earlier seasons, newest first. Loading every player is a one-time step (one request per 100 players). Update Data keeps the current season up to date.'
      : 'Not configured: set BALLDONTLIE_API_KEY in .env and restart the application.';
    document.getElementById('playerFetches').innerHTML = d.season ? table([
      { label: 'Date', value: 'game_date', left: true }, { label: 'Fetched', value: r => new Date(r.fetched_at).toLocaleString(), left: true }, { label: 'Lines', value: r => r.counts.lines },
      { label: 'Matched', value: r => r.counts.matched }, { label: 'Unmatched', value: r => r.counts.unmatched, cls: r => r.counts.unmatched ? 'warn' : '' }, { label: 'Players', value: r => r.counts.players_upserted }], d.fetches.slice().reverse()) : '<p class="muted small">Enter a season to list its fetched dates.</p>';
  }
  async function pollPlayers() {
    let st;
    try { st = await getJSON('/api/data/players/status'); }
    catch (e) { clearTimeout(playerTimer); if (lastPlayerShown === 'fetching' && !updating) playerTimer = setTimeout(pollPlayers, 3000); return; }
    lastPlayerPhase = st;
    const el = document.getElementById('playerStatus'); el.textContent = describePlayers(st);
    const quiet = planNote && st.kind && st.kind.kind === 'daily';
    el.className = 'small ' + (st.phase === 'failed' ? (quiet ? 'muted' : 'bad') : st.phase === 'done' ? 'good' : 'muted');
    const running = st.phase === 'fetching';
    const key = `${st.phase}:${st.finished_at || ''}`;
    if (!running && !updating && lastPlayerShown && lastPlayerShown.startsWith('fetching') && key !== lastPlayerShown) await Promise.all([loadPlayers(), loadHealth()]);
    lastPlayerShown = running ? 'fetching' : key;
    clearTimeout(playerTimer); if (running && !updating) playerTimer = setTimeout(pollPlayers, 1500);
  }
  async function startPlayerSync(url, body) {
    document.getElementById('importResult').textContent = '';
    try { await postJSON(url, body); lastPlayerShown = 'fetching'; await pollPlayers(); }
    catch (e) { document.getElementById('playerStatus').textContent = e.message; document.getElementById('playerStatus').className = 'small bad'; }
  }
  document.getElementById('playerBackfillAll').addEventListener('click', () => startPlayerSync('/api/data/players/backfill-all', {}));
  document.getElementById('playerLoadAll').addEventListener('click', () => startPlayerSync('/api/data/players/load', {}));
  document.getElementById('playerSeason').addEventListener('change', () => loadPlayers().catch(e => { const el = document.getElementById('playerStatus'); el.textContent = e.message; el.className = 'small bad'; }));
  let queryTimer = null;
  document.getElementById('playerQuery').addEventListener('input', () => {
    clearTimeout(queryTimer);
    const q = document.getElementById('playerQuery').value.trim();
    if (q.length < 2) { document.getElementById('playerResults').innerHTML = ''; return; }
    queryTimer = setTimeout(async () => {
      try {
        const r = await getJSON(`/api/players?q=${encodeURIComponent(q)}`);
        const rows = await Promise.all(r.players.map(async p => ({ ...p, detail: (await getJSON(`/api/players/${p.id}`)).seasons })));
        document.getElementById('playerResults').innerHTML = rows.length ? table([
          { label: 'Player', value: p => `${p.first_name} ${p.last_name}`, left: true }, { label: 'Team', value: p => p.team_code ? `${p.team_code}${p.active ? '' : ' (not on a roster)'}${p.jersey_number ? ` #${p.jersey_number}` : ''}` : (p.active ? 'active' : ''), left: true },
          { label: 'Position', value: p => p.position || '', left: true }, { label: 'Height', value: p => p.height || '', left: true },
          { label: 'Draft', value: p => p.draft_year ? `${p.draft_year} #${p.draft_number || '?'}` : '', left: true }, { label: 'College / country', value: p => [p.college, p.country].filter(Boolean).join(' · '), left: true },
          { label: 'Seasons stored', value: p => p.detail.map(s => `${label(s.season)} ${s.team_code} ${s.games} g${s.playoff_games ? ` (${s.playoff_games} po)` : ''}`).join('; ') || 'no box-score lines yet', left: true },
          { label: 'JSON', value: p => `<a href="/api/players/${p.id}/games" target="_blank">game log</a>`, raw: true, left: true }], rows) : '<p class="muted small">No player with that name.</p>';
      } catch (e) { document.getElementById('playerResults').innerHTML = `<p class="bad">${esc(e.message)}</p>`; }
    }, 300);
  });
  refreshAll().then(() => Promise.all([pollUpdate(), pollSync(), pollPlayers()]));
})();
