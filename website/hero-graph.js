// Abstract crab constellations: no image downloads, no dependencies.
(() => {
  const canvas = document.querySelector('.hero-graph');
  const button = document.querySelector('.animation-toggle');
  if (!canvas || !button) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const hero = canvas.closest('.hero');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let width = 0, height = 0, frame = 0, time = 0, previous = 0;
  let visible = true, paused = false;
  // Each crab is a small graph: shell, eyes, jointed legs and open claws.
  const points = [[-42,-16],[0,-28],[42,-16],[53,20],[25,42],[-25,42],[-53,20],[0,9],[-24,-51],[24,-51],[-72,-40],[-82,-78],[-61,-104],[-88,-101],[-106,-73],[72,-40],[82,-78],[61,-104],[88,-101],[106,-73],[-83,9],[-114,42],[-87,38],[-111,78],[-61,58],[-72,102],[83,9],[114,42],[87,38],[111,78],[61,58],[72,102]];
  const edges = [[0,1],[1,2],[2,3],[3,4],[4,5],[5,6],[6,0],[0,7],[2,7],[4,7],[5,7],[1,7],[0,8],[2,9],[0,10],[10,11],[11,12],[11,13],[13,14],[14,10],[2,15],[15,16],[16,17],[16,18],[18,19],[19,15],[6,20],[20,21],[6,22],[22,23],[5,24],[24,25],[3,26],[26,27],[3,28],[28,29],[4,30],[30,31]];
  function line(a, b, opacity) {
    ctx.strokeStyle = `rgba(116,180,213,${opacity})`;
    ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke();
  }
  function dot(p, radius, coral, alpha = .7) {
    ctx.fillStyle = coral ? `rgba(238,159,123,${alpha})` : `rgba(145,204,232,${alpha})`;
    ctx.beginPath(); ctx.arc(p[0], p[1], radius, 0, Math.PI * 2); ctx.fill();
  }
  function draw() {
    ctx.clearRect(0,0,width,height);
    const mobile = width < 760;
    const size = mobile ? .82 : Math.min(1.6, width / 1100);
    const centers = mobile ? [[-12,height*.55],[width+12,height*.52]] : [[width*.12,height*.54],[width*.88,height*.5]];
    ctx.lineWidth = 1;
    centers.forEach(([cx,cy],side) => {
      const moving = points.map(([x,y],i) => [cx+x*size+Math.sin(time*.45+i*.8+side)*4,cy+y*size+Math.cos(time*.38+i*.7+side)*4]);
      edges.forEach(([a,b],i) => {
        line(moving[a],moving[b],.32);
        if (i%9===0) {
          const t=(time*.09+i*.137)%1;
          dot([moving[a][0]+(moving[b][0]-moving[a][0])*t,moving[a][1]+(moving[b][1]-moving[a][1])*t],1.7,true,.65);
        }
      });
      moving.forEach((p,i) => dot(p,i===8||i===9?3.4:2.2,i%6===0));
    });
    const ambient = Array.from({length:22},(_,i) => [width*i/21+Math.sin(time*.13+i)*12,height*(.84+.08*Math.sin(i*1.8))+Math.sin(time*.24+i)*8]);
    ambient.forEach((p,i) => {
      if(i) line(ambient[i-1],p,.12);
      if(i>1 && i%2===0) line(ambient[i-2],p,.08);
      dot(p,i%3===0?2.3:1.3,i%5===0,.35);
    });
  }
  function running() { return !paused && !reduced.matches && visible && !document.hidden; }
  function tick(now) {
    if (!running()) { frame=0; previous=0; return; }
    if (!previous || now-previous>=32) {
      if(previous) time+=Math.min((now-previous)/1000,.1);
      previous=now; draw();
    }
    frame=requestAnimationFrame(tick);
  }
  function sync() {
    button.hidden=reduced.matches;
    button.textContent=paused?'Play animation':'Pause animation';
    button.setAttribute('aria-pressed',String(paused));
    if (running() && !frame) frame=requestAnimationFrame(tick);
    if (!running()) { cancelAnimationFrame(frame); frame=0; previous=0; draw(); }
  }
  function resize() {
    const bounds=hero.getBoundingClientRect(); width=bounds.width; height=bounds.height;
    const dpr=Math.min(devicePixelRatio||1,2);
    canvas.width=Math.round(width*dpr); canvas.height=Math.round(height*dpr);
    ctx.setTransform(dpr,0,0,dpr,0,0); draw();
  }
  button.addEventListener('click',()=>{paused=!paused;sync();});
  reduced.addEventListener('change',sync);
  document.addEventListener('visibilitychange',sync);
  new ResizeObserver(resize).observe(hero);
  new IntersectionObserver(([entry])=>{visible=entry.isIntersecting;sync();}).observe(hero);
  resize(); sync();
})();
