(() => {
  const controls=document.querySelector('.comparison-controls');
  if(!controls)return;
  controls.hidden=false;
  const search=document.querySelector('#comparison-search'),filter=document.querySelector('#comparison-filter'),language=document.querySelector('#comparison-language');
  const rows=[...document.querySelectorAll('.comparison-row')],groups=[...document.querySelectorAll('.upstream-group')];
  const index=new Map(rows.map(row=>[row,row.textContent.toLowerCase()]));
  const members=new Map(groups.map(group=>[group,[...group.querySelectorAll('.comparison-row')]]));
  const evidenceCache=new Map();
  document.addEventListener('toggle',async event=>{
    const details=event.target;
    if(!details.matches('details[data-evidence]')||!details.open||details.dataset.loaded)return;
    const target=details.querySelector('.evidence-content');target.textContent='Loading evidence…';
    try{
      const url=details.dataset.evidence;
      if(!evidenceCache.has(url))evidenceCache.set(url,fetch(url).then(r=>{if(!r.ok)throw new Error(r.status);return r.json();}));
      const bundle=await evidenceCache.get(url);
      const data=details.dataset.product==='upstream'?bundle.cases[details.dataset.case]:bundle.results[details.dataset.product][details.dataset.case];
      const pre=document.createElement('pre');pre.textContent=JSON.stringify(data,null,2);target.replaceChildren(pre);details.dataset.loaded='true';
    }catch(error){target.textContent='Unable to load inline evidence. Use the JSON download link.';evidenceCache.delete(details.dataset.evidence);}
  },true);
  let timer;
  function update(){
    const term=search.value.trim().toLowerCase();let visible=0;
    rows.forEach(row=>{
      const show=index.get(row).includes(term)&&(filter.value==='all'||row.dataset.flags.split(' ').includes(filter.value))&&(language.value==='all'||row.dataset.language===language.value);
      row.hidden=!show;if(show)visible++;
    });
    groups.forEach(group=>{
      group.hidden=!members.get(group).some(row=>!row.hidden);
      if(term||filter.value!=='all')group.open=!group.hidden;
    });
    document.querySelector('#comparison-count').textContent=`${visible.toLocaleString()} of ${rows.length.toLocaleString()} upstream scenarios match`;
  }
  search.addEventListener('input',()=>{clearTimeout(timer);timer=setTimeout(update,120);});
  [filter,language].forEach(el=>el.addEventListener('input',update));
  document.querySelector('#comparison-reset').addEventListener('click',()=>{search.value='';filter.value='all';language.value='all';groups.forEach(g=>g.open=false);update();});
  function reveal(){
    const row=document.getElementById(location.hash.slice(1));
    if(row?.classList.contains('comparison-row')){
      search.value='';filter.value='all';language.value='all';update();row.closest('.upstream-group').open=true;row.querySelector('details').open=true;row.scrollIntoView({block:'center'});
    }
  }
  window.addEventListener('hashchange',reveal);update();reveal();
})();
