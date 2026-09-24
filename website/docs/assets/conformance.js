(() => {
  const controls = document.querySelector('.comparison-controls');
  if (!controls) return;
  controls.hidden = false;
  const search = document.querySelector('#comparison-search');
  const filter = document.querySelector('#comparison-filter');
  const language = document.querySelector('#comparison-language');
  const rows = [...document.querySelectorAll('.comparison-row')];
  const corpus = [...document.querySelectorAll('.corpus-group')];
  const index = new Map([...rows, ...corpus].map(row => [row, row.textContent.toLowerCase()]));
  function update() {
    const term = search.value.trim().toLowerCase();
    let visible = 0;
    rows.forEach(row => {
      const show = index.get(row).includes(term) &&
        (filter.value === 'all' || row.dataset.flags.split(' ').includes(filter.value)) &&
        (language.value === 'all' || row.dataset.language === language.value);
      row.hidden = !show;
      if (show) visible++;
    });
    corpus.forEach(row => { row.hidden = !index.get(row).includes(term); });
    document.querySelector('#comparison-count').textContent = `${visible} of ${rows.length} comparison rows shown`;
  }
  [search, filter, language].forEach(el => el.addEventListener('input', update));
  document.querySelector('#comparison-reset').addEventListener('click', () => {
    search.value = ''; filter.value = 'all'; language.value = 'all'; update();
  });
  function revealHash() {
    const target = document.getElementById(location.hash.slice(1));
    if (target?.classList.contains('comparison-row')) {
      search.value = ''; filter.value = 'all'; language.value = 'all'; update();
      target.querySelector('details').open = true;
      target.scrollIntoView({block:'center'});
    }
  }
  window.addEventListener('hashchange', revealHash);
  update(); revealHash();
})();
