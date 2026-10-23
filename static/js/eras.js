(async () => {
  const { num, esc, getJSON, table, setStatus, labelOf } = NBA;
  const panel = document.getElementById('builder');
  if (!panel) return;
  const $ = id => document.getElementById(id);
  const DIMS = [['cba', 'CBA period'], ['rule', 'Rule period'], ['size', 'League size'], ['league', 'League'], ['decade', 'Decade'], ['format', 'Format'], ['dynasty', 'Dynasty era'], ['player', 'Player era']];
  const DIM_TIPS = { cba: 'The collective bargaining agreement in force (NBA/BAA).', rule: 'The rule period: shot clock, three-point line, zone defense, hand-check ban; the ABA on its own.', size: 'The number of teams in the league that season.', league: 'NBA/BAA or ABA.', decade: 'Decade of the season\'s start year.', format: 'The competition format: early playoffs, 16-team field, best-of-seven throughout, play-in era, NBA Cup era.', dynasty: 'A franchise\'s run of titles (thresholds in the filter row), computed for NBA/BAA champions only. Overlapping runs share a joined label; seasons outside any run are "No dynasty".', player: 'Curated careers of every league and Finals MVP (and George Mikan). A season belongs to every player active in it, so season counts add up across rows.' };
  $('dims').innerHTML = DIMS.map(([k, t]) => `<button class="chip" data-dim="${k}" aria-pressed="false" title="${esc(DIM_TIPS[k])}">${esc(t)}</button>`).join('');
  const pressed = () => [...$('dims').querySelectorAll('.chip')].filter(c => c.getAttribute('aria-pressed') === 'true').map(c => c.dataset.dim);
  const setDims = list => $('dims').querySelectorAll('.chip').forEach(c => c.setAttribute('aria-pressed', String(list.includes(c.dataset.dim))));
  let pinned = new Set(), last = null;
  const span = g => g.first_season === g.last_season ? labelOf(g.first_season) : `${labelOf(g.first_season)} to ${labelOf(g.last_season)}`;
  const titleList = g => { if (!g.champions.length) return 'none recorded'; const all = g.champions.map(t => `${t.code} ${t.titles}`); return `<span title="${esc(all.join(', '))}">${esc(all.slice(0, 4).join(', '))}${all.length > 4 ? ` +${all.length - 4} more` : ''}</span>`; };
  const query = () => {
    const p = new URLSearchParams();
    p.set('by', pressed().join(','));
    const league = $('gLeague').value; if (league !== 'all') p.set('league', league);
    if ($('gFrom').value) p.set('from', $('gFrom').value); if ($('gTo').value) p.set('to', $('gTo').value);
    if ($('gTitles').value !== '3') p.set('dynasty_titles', $('gTitles').value); if ($('gGap').value !== '3') p.set('dynasty_gap', $('gGap').value);
    return p.toString().replace(/%2C/g, ',');
  };
  function render(data) {
    last = data;
    const dims = data.query.dimensions, min = +$('gMin').value || 1;
    const header = dims.length ? dims.map(d => (DIMS.find(x => x[0] === d) || [d, d])[1]).join(' × ') : 'Group';
    const rows = data.groups;
    $('groups').innerHTML = rows.length ? table([
      { label: 'Pin', value: g => `<input type="checkbox" class="pin" data-key="${esc(g.key)}" ${pinned.has(g.key) ? 'checked' : ''} aria-label="Pin ${esc(g.key)}">`, raw: true },
      { label: header, value: g => `<span title="${esc(g.notes.map(k => data.notes[k] ? data.notes[k].summary : '').join(' / '))}">${esc(g.key)}</span>${g.notes.length ? ` <a href="#note-${rows.indexOf(g)}" class="small muted note-link" title="What changed in this period">ⓘ</a>` : ''}`, raw: true, left: true },
      { label: 'Seasons', value: g => `${g.seasons} (${span(g)})`, left: true },
      { label: 'Best team-season', value: g => `${g.best.code} ${num(g.best.value, 0)} (${labelOf(g.best.season)})`, left: true, cls: () => 'good' },
      { label: 'Weakest', value: g => `${g.weakest.code} ${num(g.weakest.value, 0)} (${labelOf(g.weakest.season)})`, left: true, cls: () => 'bad' },
      { label: 'Titles', value: 'titles' }, { label: 'Champions', value: titleList, raw: true, left: true }, { label: 'Distinct', value: 'distinct_champions' },
      { label: 'Title HHI', value: g => g.title_hhi == null ? '–' : num(g.title_hhi, 2) }, { label: 'Elo spread', value: g => g.elo_spread == null ? '–' : num(g.elo_spread, 0) }], rows) : '<p class="muted">No group matches these filters.</p>';
    $('groups').querySelectorAll('tbody tr').forEach((tr, i) => { if (rows[i] && rows[i].seasons < min) tr.classList.add('thin'); });
    const thin = rows.filter(g => g.seasons < min).length;
    $('gThin').textContent = thin ? `${thin} group${thin === 1 ? '' : 's'} with fewer than ${min} seasons shown in grey: too few seasons to compare.` : '';
    const noted = rows.filter(g => g.notes.length);
    $('groupNotes').innerHTML = noted.length ? `<h3 style="margin-top:.8rem">What changed in these periods</h3><div class="notes">${noted.map(g => `<details id="note-${rows.indexOf(g)}"><summary><strong>${esc(g.key)}</strong> · ${esc(span(g))}</summary>${g.notes.map(k => data.notes[k]).filter(Boolean).map(n => `<p class="small" style="margin:.4rem 0 .1rem"><strong>${esc(n.name)}</strong> <span class="muted">${esc(n.summary)}</span></p><ul class="small">${n.changes.map(c => `<li><span class="muted">${labelOf(c.season)}</span> ${esc(c.text)}</li>`).join('')}</ul>`).join('')}</details>`).join('')}</div>` : '';
    $('gUrl').textContent = `GET /api/records/groups?${query()}`;
    renderCompare();
  }
  function renderCompare() {
    if (!last) return;
    const picked = last.groups.filter(g => pinned.has(g.key));
    $('compare').innerHTML = picked.length >= 2 ? picked.map(g => `<div class="card"><h3>${esc(g.key)}</h3><div class="kv">
      <div>Seasons</div><div>${g.seasons} (${esc(span(g))})</div>
      <div>Best team-season</div><div>${esc(g.best.code)} ${labelOf(g.best.season)} · ${num(g.best.value, 0)}</div>
      <div>Weakest</div><div>${esc(g.weakest.code)} ${labelOf(g.weakest.season)} · ${num(g.weakest.value, 0)}</div>
      <div>Champions</div><div>${g.champions.length ? esc(g.champions.map(t => `${t.code} ${t.titles}`).join(', ')) : 'none recorded'}</div>
      <div>Distinct champions</div><div>${g.distinct_champions} of ${g.titles}</div>
      <div>Title HHI</div><div>${g.title_hhi == null ? '–' : num(g.title_hhi, 2)}</div>
      <div>Elo spread</div><div>${g.elo_spread == null ? '–' : num(g.elo_spread, 0)}</div></div></div>`).join('') : '';
  }
  async function load() {
    setStatus($('gStatus'), 'Loading…');
    try { render(await getJSON(`/api/records/groups?${query()}`)); setStatus($('gStatus'), ''); }
    catch (e) { setStatus($('gStatus'), e.message, true); }
  }
  $('presets').addEventListener('click', e => { const b = e.target.closest('.preset'); if (!b) return; setDims(b.dataset.preset.split(',')); $('gLeague').value = b.dataset.preset.includes('league') ? 'all' : (/(cba|dynasty|player)/.test(b.dataset.preset) ? 'nba' : 'all'); load(); });
  $('dims').addEventListener('click', e => { const c = e.target.closest('.chip'); if (!c) return; c.setAttribute('aria-pressed', String(c.getAttribute('aria-pressed') !== 'true')); load(); });
  $('gUpdate').addEventListener('click', load);
  ['gLeague', 'gFrom', 'gTo', 'gTitles', 'gGap'].forEach(id => $(id).addEventListener('change', load));
  $('gMin').addEventListener('change', () => last && render(last));
  // The ⓘ link opens its notes entry.
  $('groups').addEventListener('click', e => { const a = e.target.closest('a.note-link'); if (!a) return; const d = document.getElementById(a.getAttribute('href').slice(1)); if (d) d.open = true; });
  $('groups').addEventListener('change', e => { const pin = e.target.closest('input.pin'); if (!pin) return; if (pin.checked) pinned.add(pin.dataset.key); else pinned.delete(pin.dataset.key); renderCompare(); });
  $('gCopy').addEventListener('click', async () => {
    const url = `${location.origin}/records?${query()}#builder`;
    try { await navigator.clipboard.writeText(url); setStatus($('gStatus'), 'Link copied.'); } catch { setStatus($('gStatus'), url); }
  });
  // A link with a query opens that view.
  const q = new URLSearchParams(location.search);
  if (q.has('by')) {
    setDims(q.get('by').split(',').filter(Boolean));
    const league = (q.get('league') || '').toLowerCase(); if ([...$('gLeague').options].some(o => o.value === league)) $('gLeague').value = league;
    if (q.get('from')) $('gFrom').value = q.get('from'); if (q.get('to')) $('gTo').value = q.get('to');
    if (q.get('dynasty_titles')) $('gTitles').value = q.get('dynasty_titles'); if (q.get('dynasty_gap')) $('gGap').value = q.get('dynasty_gap');
  } else setDims(['cba']);
  load();
})();
