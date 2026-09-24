(() => {
  const controls=document.querySelector('.comparison-controls');if(!controls)return;controls.hidden=false;
  document.documentElement.classList.add('matrix-view');
  const search=document.querySelector('#comparison-search'),filter=document.querySelector('#comparison-filter');
  const cards=[...document.querySelectorAll('.feature-card')],rows=[...document.querySelectorAll('.comparison-row')],languages=[...document.querySelectorAll('.language-group')],cache=new Map();
  const members=new Map(cards.map(card=>[card,[...card.querySelectorAll('.comparison-row')]]));
  const index=new Map(rows.map(row=>[row,(row.textContent+' '+row.closest('.feature-card').dataset.name+' '+row.querySelector('[data-case]').dataset.case).toLowerCase()]));
  let timer;
  function update(){
    const term=search.value.trim().toLowerCase();let visible=0;
    for(const row of rows)row.hidden=!(index.get(row).includes(term)&&(filter.value==='all'||row.dataset.flags.split(' ').includes(filter.value)));
    for(const card of cards){card.hidden=!members.get(card).some(row=>!row.hidden);if(!card.hidden)visible++;}
    for(const section of languages)section.hidden=![...section.querySelectorAll('.feature-card')].some(card=>!card.hidden);
    document.querySelector('#comparison-count').textContent=visible+' of '+cards.length+' features';
    document.querySelector('#empty-stage').hidden=visible>0;
  }
  function reset(){search.value='';filter.value='all';update();}
  search.addEventListener('input',()=>{clearTimeout(timer);timer=setTimeout(update,120);});filter.addEventListener('change',update);
  document.querySelector('#comparison-reset').addEventListener('click',reset);
  function reveal(){
    const target=document.getElementById(location.hash.slice(1));if(!target)return;
    const card=target.closest('.feature-card');
    if(target.hidden||card?.hidden)reset();
    if(target.matches('details'))target.open=true;
    if(target.classList.contains('comparison-row')){card.querySelector('.upstream-group').open=true;target.querySelector('details').open=true;}
    target.scrollIntoView({block:'start'});
  }
  document.addEventListener('click',event=>{
    const cell=event.target.closest('[data-product-focus]');if(!cell)return;
    const card=cell.closest('.feature-card');card.dataset.focus=cell.dataset.productFocus;
    const tests=card.querySelector('.upstream-group');tests.open=true;
    const result=tests.querySelector('.comparison-row:not([hidden]) [data-product="'+cell.dataset.productFocus+'"]');if(result)result.open=true;
  });
  function block(parent,title,data){
    if(data===undefined||data===null||data==='')return;
    const heading=document.createElement('h4');heading.textContent=title;
    const pre=document.createElement('pre');pre.textContent=typeof data==='string'?data:JSON.stringify(data,null,2);parent.append(heading,pre);
  }
  function evidence(target,data,upstream){
    target.replaceChildren();
    if(upstream&&data.steps){
      for(const step of data.steps){
        const heading=document.createElement('h4');heading.textContent=step.text;target.append(heading);
        if(step.doc){const pre=document.createElement('pre');pre.textContent=step.doc;target.append(pre);}
        if(step.table){const table=document.createElement('table');table.className='expectation-table';for(const values of step.table){const row=document.createElement('tr');for(const value of values){const cell=document.createElement('td');cell.textContent=value;row.append(cell);}table.append(row);}target.append(table);}
      }
    }else if(upstream){block(target,'Query / manifest',data);}
    else{
      block(target,'Diagnostic',data.reason||data.error);block(target,'Failed step',data.failed_step);
      block(target,'Query',data.query);block(target,'Expected',data.expected||data.expected_error);
      let actual=data.actual??data.actual_graphson;
      if(typeof actual==='string'){try{actual=JSON.parse(actual);}catch{}}
      block(target,'Actual result',actual);
      if(data.elapsed_ms!==undefined){const timing=document.createElement('p');timing.textContent=data.elapsed_ms.toLocaleString()+' ms total scenario time';target.append(timing);}
    }
    const raw=document.createElement('details'),summary=document.createElement('summary'),pre=document.createElement('pre');summary.textContent='Raw evidence JSON';pre.className='raw-evidence';pre.textContent=JSON.stringify(data,null,2);raw.append(summary,pre);target.append(raw);
  }
  document.addEventListener('toggle',async event=>{
    const details=event.target;
    if(!details.matches('details[data-evidence]')||!details.open||details.dataset.loaded)return;
    const target=details.querySelector('.evidence-content');target.textContent='Loading evidence…';
    try{
      const url=details.dataset.evidence;
      if(!cache.has(url))cache.set(url,fetch(url).then(r=>{if(!r.ok)throw new Error(r.status);return r.json();}));
      const bundle=await cache.get(url),upstream=details.dataset.product==='upstream';
      const data=upstream?bundle.cases[details.dataset.case]:bundle.results[details.dataset.product][details.dataset.case];
      evidence(target,data,upstream);details.dataset.loaded='true';
    }catch(error){target.textContent='Unable to load inline evidence. Use the JSON download link.';cache.delete(details.dataset.evidence);}
  },true);
  window.addEventListener('hashchange',reveal);update();reveal();
})();
