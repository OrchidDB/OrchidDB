(() => {
  const controls=document.querySelector('.comparison-controls');if(!controls)return;controls.hidden=false;
  document.documentElement.classList.add('matrix-view');
  const search=document.querySelector('#comparison-search'),filter=document.querySelector('#comparison-filter'),loweringFilter=document.querySelector('#lowering-filter');
  const cards=[...document.querySelectorAll('.feature-card')],rows=[...document.querySelectorAll('.comparison-row')],languages=[...document.querySelectorAll('.language-group')],cache=new Map();
  const members=new Map(cards.map(card=>[card,[...card.querySelectorAll('.comparison-row')]]));
  const detailRows=new Map();
  const index=new Map(rows.map(row=>[row,(row.textContent+' '+row.closest('.feature-card').dataset.name+' '+row.querySelector('[data-case]').dataset.case).toLowerCase()]));
  let timer;
  function update(){
    const term=search.value.trim().toLowerCase();let visible=0;
    for(const row of rows)row.hidden=!(index.get(row).includes(term)&&(filter.value==='all'||row.dataset.flags.split(' ').includes(filter.value))&&(loweringFilter.value==='all'||row.dataset.lowering===loweringFilter.value));
    for(const [row,detail] of detailRows)detail.hidden=row.hidden;
    for(const card of cards){card.hidden=!members.get(card).some(row=>!row.hidden);if(!card.hidden)visible++;}
    for(const section of languages)section.hidden=![...section.querySelectorAll('.feature-card')].some(card=>!card.hidden);
    document.querySelector('#comparison-count').textContent=visible+' of '+cards.length+' features';
    document.querySelector('#empty-stage').hidden=visible>0;
  }
  function reset(){search.value='';filter.value='all';loweringFilter.value='all';update();}
  search.addEventListener('input',()=>{clearTimeout(timer);timer=setTimeout(update,120);});filter.addEventListener('change',update);
  loweringFilter.addEventListener('change',update);
  document.querySelector('#comparison-reset').addEventListener('click',reset);
  function reveal(){
    const target=document.getElementById(location.hash.slice(1));if(!target)return;
    const card=target.closest('.feature-card');
    if(target.hidden||card?.hidden)reset();
    if(target.matches('details'))target.open=true;
    if(target.classList.contains('comparison-row')){card.querySelector('.upstream-group').open=true;const result=target.querySelector('[data-product="orchiddb"]');if(result.getAttribute('aria-expanded')!=='true')result.click();}
    target.scrollIntoView({block:'start'});
  }
  document.addEventListener('click',event=>{
    const cell=event.target.closest('[data-product-focus]');if(!cell)return;
    const card=cell.closest('.feature-card');card.dataset.focus=cell.dataset.productFocus;
    const tests=card.querySelector('.upstream-group');tests.open=true;
    tests.scrollIntoView({block:'nearest'});
  });
  let nextPanel=0;
  document.addEventListener('click',async event=>{
    const button=event.target.closest('button[data-evidence]');if(!button)return;
    const row=button.closest('.comparison-row'),upstream=button.dataset.product==='upstream';
    let detail=detailRows.get(row);
    if(detail?.activeButton===button){
      button.setAttribute('aria-expanded','false');button.removeAttribute('aria-controls');detail.remove();detailRows.delete(row);return;
    }
    if(!detail){
      detail=document.createElement('tr');detail.className='case-detail-row';
      const cell=document.createElement('td');cell.colSpan=row.children.length;
      detail.append(cell);row.after(detail);detailRows.set(row,detail);
    }
    detail.activeButton?.setAttribute('aria-expanded','false');detail.activeButton?.removeAttribute('aria-controls');detail.activeButton=button;
    const panel=document.createElement('section');panel.className='case-details';panel.id='case-details-'+(++nextPanel);
    button.setAttribute('aria-controls',panel.id);button.setAttribute('aria-expanded','true');
    const header=document.createElement('header');header.className='case-details-header';
    const title=document.createElement('h3');title.id=panel.id+'-title';
    title.textContent=(upstream?'Scenario':button.dataset.productName)+' · '+row.querySelector('th>a').textContent;
    panel.setAttribute('aria-labelledby',title.id);
    const close=document.createElement('button');close.type='button';close.textContent='Collapse details';
    close.addEventListener('click',()=>{button.setAttribute('aria-expanded','false');button.removeAttribute('aria-controls');detail.remove();detailRows.delete(row);button.focus();});
    header.append(title,close);
    const content=document.createElement('div');content.className='evidence-content';content.setAttribute('aria-live','polite');
    content.textContent='Loading case details…';panel.append(header,content);detail.firstElementChild.replaceChildren(panel);
    row.closest('.comparison-scroll').scrollLeft=0;row.closest('.feature-matrix-scroll').scrollLeft=0;
    try{
      const url=button.dataset.evidence;
      if(!cache.has(url))cache.set(url,fetch(url).then(r=>{if(!r.ok)throw new Error(r.status);return r.json();}));
      const bundle=await cache.get(url);if(!panel.isConnected)return;
      const scenario=bundle.cases[button.dataset.case];
      const data=upstream?scenario:bundle.results[button.dataset.product][button.dataset.case];
      window.OrchidEvidence.render(content,data,upstream,button.dataset.product,scenario);
    }catch(error){content.textContent='Unable to load case details. Collapse this section and select the result to retry.';cache.delete(button.dataset.evidence);}
  });
  window.addEventListener('hashchange',reveal);update();reveal();
})();
