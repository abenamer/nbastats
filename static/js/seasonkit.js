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
  function homeBonusNote(seasonSelect, radv, note) {
    let request = 0;
    const show = async () => {
      const season = parseInt(seasonSelect.value);
      const mine = ++request;
      const h = await getJSON(`/api/seasons/${season}/home-advantage`).catch(() => null);
      if (mine !== request) return;
      if (!h) { radv.placeholder = '100'; note.textContent = ''; return; }
      radv.placeholder = num(h.regular, 0);
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
  Object.assign(NBA, { sourceName, whatIfSentence, bonusText, teamLink, query, homeBonusNote, playerSearch, whatIfControl });
})();
