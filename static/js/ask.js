(async () => {
  const { esc, getJSON, postJSON, setStatus } = NBA;
  const panel = document.getElementById('askPanel');
  if (!panel) return;
  const input = document.getElementById('askInput'), button = document.getElementById('askButton');
  const status = document.getElementById('askStatus'), out = document.getElementById('askAnswer');
  const steps = document.getElementById('askSteps'), hint = document.getElementById('askHint');
  try {
    const s = await getJSON('/api/ask/status');
    if (!s.enabled) {
      input.disabled = true; button.disabled = true;
      hint.textContent = 'Not enabled: set OPENROUTER_API_KEY in .env (and OPENROUTER_MODEL to choose the model) to turn the question box on.';
      return;
    }
    input.maxLength = s.max_question_chars;
    hint.textContent = `Answers come from ${s.model} through OpenRouter, from lookups in this application's data. Your question and the data it needs leave this computer. Check the numbers against the pages; the model can misread them.`;
  } catch (e) { hint.textContent = e.message; return; }
  async function ask() {
    const q = input.value.trim();
    if (!q) return;
    button.disabled = true; input.disabled = true;
    setStatus(status, 'Looking it up…'); out.textContent = ''; steps.innerHTML = '';
    try {
      const a = await postJSON('/api/ask', { question: q });
      out.textContent = a.answer;
      out.classList.toggle('muted', a.declined);
      const tokens = (a.input_tokens + a.output_tokens).toLocaleString();
      steps.innerHTML = a.steps.length
        ? `<details><summary>${a.steps.length} lookup${a.steps.length === 1 ? '' : 's'} · ${esc(a.model)} · ${tokens} tokens</summary><ul>${a.steps.map(s => `<li><code>${esc(s.tool)}</code> ${esc(JSON.stringify(s.arguments))} → ${esc(s.summary)}</li>`).join('')}</ul></details>`
        : `<span class="muted">${esc(a.model)} · no lookup · ${tokens} tokens</span>`;
      setStatus(status, '');
    } catch (e) { setStatus(status, e.message, true); }
    finally { button.disabled = false; input.disabled = false; input.focus(); }
  }
  button.addEventListener('click', ask);
  input.addEventListener('keydown', e => { if (e.key === 'Enter') ask(); });
})();
