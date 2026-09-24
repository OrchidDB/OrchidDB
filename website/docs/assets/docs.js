document.documentElement.classList.add('js');
const menu = document.querySelector('#menu');
menu.hidden = false;
menu.addEventListener('click', () => {
  const open = menu.getAttribute('aria-expanded') !== 'true';
  menu.setAttribute('aria-expanded', String(open));
  document.querySelector('#sidebar').classList.toggle('open', open);
});
document.querySelectorAll('.copy').forEach(button => button.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(button.closest('.code-block').querySelector('code').textContent);
    button.textContent = 'Copied';
    document.querySelector('#copy-status').textContent = 'Code copied to clipboard';
    setTimeout(() => { button.textContent = 'Copy'; }, 1800);
  } catch { button.textContent = 'Select code'; }
}));
const dialog = document.querySelector('#search-dialog');
const input = document.querySelector('#search-input');
const results = document.querySelector('#search-results');
const status = document.querySelector('#search-status');
const trigger = document.querySelector('#search-open');
let index;
trigger.hidden = false;
async function openSearch() {
  if (dialog.open) return;
  dialog.showModal();
  input.focus();
  status.textContent = 'Loading search…';
  try {
    if (!index) {
      const response = await fetch('/search-index.json');
      if (!response.ok) throw new Error('Search unavailable');
      index = await response.json();
    }
    search();
  } catch { status.textContent = 'Search could not load. Browse pages in the navigation or try again.'; }
}
function search() {
  if (!index) return;
  const terms = input.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const matches = index.map(page => {
    const title = page.title.toLowerCase(), haystack = `${title} ${page.description} ${page.text}`.toLowerCase();
    const score = terms.every(term => haystack.includes(term)) ? terms.reduce((n,term) => n + (title.includes(term) ? 10 : 1), 0) : -1;
    return {page,score};
  }).filter(hit => hit.score >= 0).sort((a,b) => b.score-a.score).slice(0,12);
  results.replaceChildren();
  status.textContent = terms.length ? `${matches.length} result${matches.length === 1 ? '' : 's'}${matches.length ? '' : '. Try a language, API name, or topic.'}` : 'Browse the guides or type to search all documentation.';
  for (const {page} of matches) {
    const a = document.createElement('a'); a.href = page.url; a.className = 'search-result';
    const group = document.createElement('small'); group.textContent = page.group;
    const title = document.createElement('strong'); title.textContent = page.title;
    const p = document.createElement('p'); p.textContent = page.description;
    a.append(group,title,p); results.append(a);
  }
}
trigger.addEventListener('click',openSearch);
document.querySelector('#search-close').addEventListener('click',() => dialog.close());
dialog.addEventListener('click',event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dialog.close(); } });
input.addEventListener('input',search);
input.addEventListener('keydown',event => { if (event.key === 'ArrowDown') { event.preventDefault(); results.querySelector('a')?.focus(); } });
document.addEventListener('keydown',event => {
  if ((event.key === '/' && !/INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName)) || ((event.metaKey || event.ctrlKey) && event.key === 'k')) { event.preventDefault(); openSearch(); }
  if (event.key === 'Escape') { if (dialog.open) { event.preventDefault(); dialog.close(); } menu.setAttribute('aria-expanded','false'); document.querySelector('#sidebar').classList.remove('open'); }
});
if ('IntersectionObserver' in window) {
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) if (entry.isIntersecting) {
      document.querySelectorAll('.toc a').forEach(a => a.classList.toggle('active',a.hash === '#'+entry.target.id));
    }
  }, {rootMargin:'-90px 0px -65% 0px'});
  document.querySelectorAll('h2[id]').forEach(h => observer.observe(h));
}
