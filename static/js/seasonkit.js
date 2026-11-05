// The pieces the Season pages share: the player search, the worth what-if control, the
// home-bonus note, the team link to the injury card and the wording of a what-if. Loaded
// after app.js and before a page's own script; everything is added to NBA.
(() => {
  const { num, esc, getJSON, setStatus, labelOf } = NBA;
  const sourceName = s => ({ worth: 'missed games', box: 'box score', blend: 'missed games and box score', draft: 'draft prior' })[s] || s || 'estimate';
  // The what-ifs an answer was priced with, in words, for the Tonight note and the card.
  const whatIfSentence = ws => (ws || []).length ? ` What-if: ${ws.map(w => `${w.name} at ${num(w.elo, 0)} Elo (${w.basis ? `${w.basis}; ` : ''}the model's estimate: ${w.current_elo == null ? 'none' : `${num(w.current_elo, 0)} from the ${sourceName(w.current_source)}`}), so ${w.team_code} ${w.shift_elo >= 0 ? '+' : '−'}${num(Math.abs(w.shift_elo), 0)}`).join('; ')}.` : '';
  const bonusText = (h, fitted) => Math.abs(h - fitted) < 0.05 ? `Home bonus ${num(h, 0)} (fitted)` : `Home bonus ${num(h, 0)} (yours; fitted ${num(fitted, num(h, 0) === num(fitted, 0) ? 1 : 0)})`;
  // A team code that opens the team's injury card page for the season.
  const teamLink = (code, season) => `<a href="/season/injuries?season=${encodeURIComponent(season)}&team=${encodeURIComponent(code)}" title="Open the team's injury card">${esc(code)}</a>`;
  const query = (...parts) => { const q = parts.filter(Boolean).join('&'); return q ? `?${q}` : ''; };
  // The regular-season home-court bonus is fitted per season; the field shows the fit as its
  // placeholder and a typed value replaces it. Playoff games keep the archive's 100.
  // The home-court note of a season; `radv` is the bonus input whose placeholder takes the
  // fitted value, or null on a page without one.
  function homeBonusNote(seasonSelect, radv, note) {
    let request = 0;
    const show = async () => {
      const season = parseInt(seasonSelect.value);
      const mine = ++request;
      const h = await getJSON(`/api/seasons/${season}/home-advantage`).catch(() => null);
      if (mine !== request) return;
      if (!h) { if (radv) radv.placeholder = '100'; note.textContent = ''; return; }
      if (radv) radv.placeholder = num(h.regular, 0);
      note.textContent = h.fitted_from
        ? `Regular-season home-court bonus for ${labelOf(season)}: ${num(h.regular, 0)} Elo, fitted on ${h.games.toLocaleString()} regular-season games of ${labelOf(h.fitted_from[0])} to ${labelOf(h.fitted_from[1])}. Playoff games use ${num(h.playoff, 0)}.`
        : `Regular-season home-court bonus for ${labelOf(season)}: ${num(h.regular, 0)} Elo (the default; fewer than 2,000 earlier regular-season games are stored). Playoff games use ${num(h.playoff, 0)}.`;
    };
    seasonSelect.addEventListener('change', show);
    show();
    return show;
  }
  // The player search behind a name field: the datalist fills from the store after two
  // letters, and `found` maps each listed name to its player. Two players of one name are
  // told apart by team or identifier in the label.
  function playerSearch(datalist, statusEl) {
    let found = new Map();
    let timer = null;
    const onInput = e => {
      const q = e.target.value.trim();
      clearTimeout(timer);
      if (q.length < 2) return;
      timer = setTimeout(async () => {
        try {
          const r = await getJSON(`/api/players?q=${encodeURIComponent(q)}`);
          const names = new Map(); r.players.forEach(p => { const n = `${p.first_name} ${p.last_name}`.trim(); names.set(n, (names.get(n) || 0) + 1); });
          found = new Map(r.players.map(p => { const n = `${p.first_name} ${p.last_name}`.trim(); return [names.get(n) > 1 ? `${n} (${p.team_code || `#${p.id}`})` : n, p]; }));
          datalist.innerHTML = [...found.keys()].map(n => `<option value="${esc(n)}"></option>`).join('');
        } catch (err) { if (statusEl) setStatus(statusEl, err.message, true); }
      }, 250);
    };
    return { onInput, found: () => found };
  }
  // The worth what-if control (#wiName, #wiBasis, #wiElo, #wiAdd, #wiStatus, #whatIfs): the
  // entries kept on the page, the request parameter they make, and a callback when they
  // change. At most sixteen; a second entry for a player replaces the first.
  function whatIfControl(search, onChange) {
    const entries = [];
    const name = document.getElementById('wiName'), basis = document.getElementById('wiBasis'), elo = document.getElementById('wiElo');
    const status = document.getElementById('wiStatus'), list = document.getElementById('whatIfs');
    const param = () => entries.length ? `worth=${encodeURIComponent(entries.map(w => `${w.player_id}:${w.last_games ? `last${w.last_games}` : w.elo}`).join(','))}` : '';
    const overrides = () => entries.map(w => w.last_games ? { player_id: w.player_id, last_games: w.last_games } : { player_id: w.player_id, elo: w.elo });
    const render = () => {
      list.innerHTML = entries.map((w, i) =>
        `<li>What-if: ${esc(w.name)} ${w.last_games ? `as in his ${w.last_games === 82 ? 'season so far' : w.last_games === 1 ? 'last game' : `last ${w.last_games} games`}` : `worth ${num(w.elo, 0)} Elo`} <button type="button" data-remove-whatif="${i}" class="small">remove</button></li>`).join('');
    };
    list.addEventListener('click', e => {
      const b = e.target.closest('button[data-remove-whatif]'); if (!b) return;
      entries.splice(Number(b.dataset.removeWhatif), 1); render(); onChange();
    });
    name.addEventListener('input', search.onInput);
    basis.addEventListener('change', e => { document.getElementById('wiEloLabel').hidden = e.target.value !== 'elo'; });
    document.getElementById('wiAdd').addEventListener('click', () => {
      const who = name.value.trim();
      const player = search.found().get(who);
      if (!player) { setStatus(status, 'Pick a player from the list first.', true); return; }
      let entry;
      if (basis.value === 'elo') {
        const v = parseFloat(elo.value);
        if (!Number.isFinite(v) || Math.abs(v) > 400) { setStatus(status, 'The worth must be a number of at most 400 Elo either way.', true); return; }
        entry = { name: who, player_id: player.id, elo: v };
      } else entry = { name: who, player_id: player.id, last_games: parseInt(basis.value.slice(4)) };
      const existing = entries.findIndex(w => w.player_id === player.id);
      if (existing >= 0) { entries[existing] = entry; setStatus(status, `${who} replaced.`); } else if (entries.length >= 16) { setStatus(status, 'At most 16 what-ifs.', true); return; } else { entries.push(entry); setStatus(status, ''); }
      render(); onChange();
    });
    return { entries, param, overrides };
  }
  // The players-out control (#whoName, #whoFrom, #whoTo, #whoEstimate, #whoAdd, #whoStatus,
  // #absences): absences the reader adds, each priced from the player's worth of the season,
  // one span per player. `cutoff()` gives the default start of a span (the as-of date, or
  // nothing for the preseason).
  function absenceControl(seasonSelect, search, cutoff) {
    const absences = [];
    const whoStatus = document.getElementById('whoStatus');
    const list = document.getElementById('absences');
    const seasonSpan = () => { const s = parseInt(seasonSelect.value); return [`${s - 1}-10-01`, `${s}-06-30`]; };
    const render = () => {
      list.innerHTML = absences.map((a, i) =>
        `<li>${esc(a.name)} (${esc(a.team_code)}) out ${esc(a.from)} to ${esc(a.to)}: −${num(a.elo, 0)} Elo${a.se_elo ? ` ± ${num(a.se_elo, 0)}` : ''} from the ${esc(a.source)} <button type="button" data-remove="${i}" class="small">remove</button></li>`).join('');
    };
    list.addEventListener('click', e => {
      const b = e.target.closest('button[data-remove]'); if (!b) return;
      absences.splice(Number(b.dataset.remove), 1); render();
    });
    document.getElementById('whoName').addEventListener('input', search.onInput);
    document.getElementById('whoAdd').addEventListener('click', async () => {
      const name = document.getElementById('whoName').value.trim();
      const player = search.found().get(name);
      if (!player) { setStatus(whoStatus, 'Pick a player from the list first.', true); return; }
      const season = parseInt(seasonSelect.value);
      const [start, end] = seasonSpan();
      const from = document.getElementById('whoFrom').value || cutoff() || start;
      const to = document.getElementById('whoTo').value || end;
      if (to < from) { setStatus(whoStatus, 'The span ends before it starts.', true); return; }
      const want = document.getElementById('whoEstimate').value;
      setStatus(whoStatus, 'pricing…');
      try {
        const w = await getJSON(`/api/players/${player.id}/worth`);
        const worth = w.seasons.find(s => s.season === season) || null;
        const box = (w.box_seasons || []).filter(b => b.season === season).map(b => b.row).sort((a, b) => b.minutes - a.minutes)[0] || null;
        let pick = null;
        if (want === 'worth' || (want === 'auto' && worth)) pick = worth ? { elo: worth.worth_elo, se_elo: worth.se_elo, team_id: worth.teams[worth.teams.length - 1], source: 'missed games' } : null;
        else if (box) pick = { elo: box.worth_elo, se_elo: 0, team_id: box.team_id, source: 'box score' };
        if (!pick) { setStatus(whoStatus, `No ${want === 'box' ? 'box-score' : want === 'worth' ? 'with/without' : ''} estimate for ${name} in ${labelOf(season)}.`, true); return; }
        if (pick.elo <= 0) { setStatus(whoStatus, `${name}'s estimate is ${num(pick.elo, 0)} Elo: an absence that costs nothing is not added.`, true); return; }
        const teamCode = (w.teams || {})[String(pick.team_id)] || player.team_code || `team ${pick.team_id}`;
        const entry = { name, player_id: player.id, team_id: pick.team_id, team_code: teamCode, from, to, elo: Math.min(pick.elo, 400), se_elo: Math.min(pick.se_elo, 400), source: pick.source };
        // One span per player: a second Add replaces the first entry.
        const existing = absences.findIndex(a => a.player_id === player.id);
        if (existing >= 0) { absences[existing] = entry; setStatus(whoStatus, `${name} replaced.`); } else { absences.push(entry); setStatus(whoStatus, ''); }
        render();
      } catch (err) { setStatus(whoStatus, err.message, true); }
    });
    const params = () => absences.map(a => ({ team_id: a.team_id, from: a.from, to: a.to, elo: a.elo, se_elo: a.se_elo, player_id: a.player_id }));
    return { entries: absences, params };
  }
  Object.assign(NBA, { sourceName, whatIfSentence, bonusText, teamLink, query, homeBonusNote, playerSearch, whatIfControl, absenceControl });
})();
