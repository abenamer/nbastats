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
    if (c.seasons.length && !document.getElementById('playerSeason').value) document.getElementById('playerSeason').value = c.seasons[c.seasons.length - 1].season;
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
    ['playerBackfill', 'playerBackfillAll', 'playerInjuries', 'playerRosters', 'playerLoadAll'].forEach(id => { document.getElementById(id).disabled = !configured; });
    const left = d.backfill.filter(b => b.dates_fetched < b.dates_total);
    const datesLeft = left.reduce((n, b) => n + b.dates_total - b.dates_fetched, 0);
    const loop = d.backfill_from ? ` The background backfill from ${label(d.backfill_from)} is on (PLAYER_BACKFILL_FROM) and runs season by season until nothing is left.` : (configured ? ' Set PLAYER_BACKFILL_FROM to a season end year to let the application backfill every season on its own, across restarts.' : '');
    document.getElementById('playerBackfillSummary').textContent = d.backfill.length
      ? (left.length ? `Box scores still missing in ${left.length} of ${d.backfill.length} seasons (${datesLeft.toLocaleString()} dates): ${left.slice(-6).map(b => `${label(b.season)} ${b.dates_fetched}/${b.dates_total}`).join(', ')}${left.length > 6 ? ', …' : ''}.` : `Every played date of all ${d.backfill.length} seasons has its box scores.`) + loop
      : '';
    document.getElementById('playerHint').textContent = configured
      ? (d.daily_sync ? `The daily results sync is followed by the player sync: the season's unfetched dates, the last ${d.settling_days} days again, today's injury report and the rosters. Loading every player is a one-time step (one request per 100 players).` : 'No daily sync (RESULTS_SYNC_ENABLED or PLAYER_SYNC_ENABLED is false); the buttons fetch now.')
      : 'Not configured: set BALLDONTLIE_API_KEY in .env and restart the application.';
    document.getElementById('playerFetches').innerHTML = d.season ? table([
      { label: 'Date', value: 'game_date', left: true }, { label: 'Fetched', value: r => new Date(r.fetched_at).toLocaleString(), left: true }, { label: 'Lines', value: r => r.counts.lines },
      { label: 'Matched', value: r => r.counts.matched }, { label: 'Unmatched', value: r => r.counts.unmatched, cls: r => r.counts.unmatched ? 'warn' : '' }, { label: 'Players', value: r => r.counts.players_upserted }], d.fetches.slice().reverse()) : '<p class="muted small">Enter a season to list its fetched dates.</p>';
  }
  async function pollPlayers() {
    let st; try { st = await getJSON('/api/data/players/status'); } catch (e) { return; }
    const el = document.getElementById('playerStatus'); el.textContent = describePlayers(st);
    el.className = 'small ' + (st.phase === 'failed' ? 'bad' : st.phase === 'done' ? 'good' : '');
    const running = st.phase === 'fetching';
    const key = `${st.phase}:${st.finished_at || ''}`;
    if (!running && lastPlayerShown && lastPlayerShown.startsWith('fetching') && key !== lastPlayerShown) await Promise.all([loadPlayers(), loadHealth()]);
    lastPlayerShown = running ? 'fetching' : key;
    clearTimeout(playerTimer); if (running) playerTimer = setTimeout(pollPlayers, 1500);
  }
  async function startPlayerSync(url, body) {
    document.getElementById('importResult').textContent = '';
    try { await postJSON(url, body); lastPlayerShown = 'fetching'; await pollPlayers(); }
    catch (e) { document.getElementById('playerStatus').textContent = e.message; document.getElementById('playerStatus').className = 'small bad'; }
  }
  document.getElementById('playerBackfill').addEventListener('click', () => {
    const season = parseInt(document.getElementById('playerSeason').value);
    if (!season) { document.getElementById('playerStatus').textContent = 'Enter the season to backfill.'; return; }
    startPlayerSync('/api/data/players/backfill', { season });
  });
  document.getElementById('playerBackfillAll').addEventListener('click', () => startPlayerSync('/api/data/players/backfill-all', {}));
  document.getElementById('playerInjuries').addEventListener('click', () => startPlayerSync('/api/data/players/injuries', {}));
  document.getElementById('playerRosters').addEventListener('click', () => startPlayerSync('/api/data/players/rosters', {}));
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
  refreshAll().then(() => Promise.all([pollSync(), pollPlayers()]));
})();
