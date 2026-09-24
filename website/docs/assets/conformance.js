(() => {
  const controls=document.querySelector('.comparison-controls');
  if(!controls)return;
  controls.hidden=false;
  document.querySelector('.feature-index').hidden=false;
  document.documentElement.classList.add('feature-explorer');
  const search=document.querySelector('#comparison-search'),filter=document.querySelector('#comparison-filter');
  const cards=[...document.querySelectorAll('.feature-card')],rows=[...document.querySelectorAll('.comparison-row')];
  const members=new Map(cards.map(card=>[card,[...card.querySelectorAll('.comparison-row')]]));
  const index=new Map(rows.map(row=>[row,(row.textContent+' '+row.closest('.feature-card').dataset.name+' '+row.querySelector('[data-case]').dataset.case).toLowerCase()]));
  const list=document.querySelector('#feature-list'),links=new Map(),cache=new Map();
  const mobileSelect=document.createElement('select');mobileSelect.id='mobile-feature-select';mobileSelect.setAttribute('aria-label','Select a feature');list.before(mobileSelect);
  let selected, timer, language='tinkerpop';
  const tabs=[...document.querySelectorAll('[data-language-tab]')];
  const languages=[...document.querySelectorAll('.language-group')];
  function preferred(){return cards.find(card=>card.dataset.suite===language&&['count()','Count','Basic'].includes(card.dataset.name))||cards.find(card=>card.dataset.suite===language);}
  for(const card of cards){
    const link=document.createElement('a');link.href='#'+card.id;
    const name=document.createElement('span');name.textContent=card.dataset.name;
    const meta=document.createElement('small');meta.textContent=card.dataset.languageName+' · '+members.get(card).length+' scenarios';
    link.append(name,meta);list.append(link);links.set(card,link);
  }
  function select(card,scroll=false){
    selected=card;mobileSelect.value=card?.id||'';
    for(const section of languages)section.hidden=section.dataset.suite!==language;
    for(const tab of tabs)tab.setAttribute('aria-current',String(tab.dataset.languageTab===language));
    for(const item of cards){item.hidden=item!==card;links.get(item).setAttribute('aria-current',String(item===card));}
    if(card&&(search.value.trim()||filter.value!=='all'))card.querySelector('.upstream-group').open=true;
    if(card){const link=links.get(card);list.scrollTop=link.offsetTop-list.offsetTop-list.clientHeight/3;}
    if(scroll&&card)card.scrollIntoView({block:'start'});
  }
  function update(){
    for(const option of filter.options){if(['differences','crab-wins','peer-wins'].includes(option.value)){option.hidden=option.disabled=language==='rdf';if(option.disabled&&option.selected)filter.value='all';}}
    const defaultCard=preferred();
    const term=search.value.trim().toLowerCase();let matched=0;const eligible=[];
    for(const row of rows){
      row.hidden=!(index.get(row).includes(term)&&(filter.value==='all'||row.dataset.flags.split(' ').includes(filter.value))&&row.dataset.language===language);
      if(!row.hidden)matched++;
    }
    for(const card of cards){const show=members.get(card).some(row=>!row.hidden);links.get(card).hidden=!show;if(show)eligible.push(card);}
    mobileSelect.replaceChildren(...eligible.map(card=>new Option(card.dataset.name,card.id)));
    document.querySelector('#comparison-count').textContent=eligible.length+' / '+cards.filter(card=>card.dataset.suite===language).length;
    document.querySelector('#empty-features').hidden=eligible.length>0;
    document.querySelector('#empty-stage').hidden=eligible.length>0;
    select(eligible.includes(selected)?selected:eligible.includes(defaultCard)?defaultCard:eligible[0]);
    controls.dataset.matchingCases=String(matched);
  }
  mobileSelect.addEventListener('change',()=>{const card=cards.find(card=>card.id===mobileSelect.value);select(card);history.pushState(null,'','#'+card.id);});
  search.addEventListener('input',()=>{clearTimeout(timer);timer=setTimeout(update,120);});
  filter.addEventListener('change',update);
  for(const tab of tabs)tab.addEventListener('click',event=>{event.preventDefault();language=tab.dataset.languageTab;reset();selected=preferred();update();history.pushState(null,'',tab.getAttribute('href'));});
  function reset(){search.value='';filter.value='all';}
  document.querySelector('#comparison-reset').addEventListener('click',()=>{reset();selected=preferred();cards.forEach(card=>card.querySelector('.upstream-group').open=false);update();history.replaceState(null,'','#'+selected.id);});
  function reveal(){
    const target=document.getElementById(location.hash.slice(1));if(!target)return;
    if(target.classList.contains('language-group')){language=target.dataset.suite;reset();selected=preferred();update();return;}
    const card=target.closest('.feature-card');
    if(card){
      if(language!==card.dataset.suite){language=card.dataset.suite;reset();update();}
      if(links.get(card).hidden||target.classList.contains('comparison-row')&&target.hidden){reset();update();}
      select(card);
      if(target.classList.contains('comparison-row')){card.querySelector('.upstream-group').open=true;target.querySelector('details').open=true;target.scrollIntoView({block:'center'});}
      else if(target.matches('details')){target.open=true;target.scrollIntoView({block:'start'});}
    }else if(target.matches('details'))target.open=true;
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
