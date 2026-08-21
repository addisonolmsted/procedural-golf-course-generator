import json, pathlib
SP = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad")
DATA = (SP/"t1_data.json").read_text().strip()
OUT = SP/"t1_explorer.html"

# per-archetype presets from the records (mid-range values)
PRESETS = {
 "Piedmont":     dict(exp=0.95, r400=14, fhw=32, widen=1.2, budget=50, terr=0, tread=120, riserh=8, bperiod=0, bk=0.1, w_int=0.35),
 "Great Plains": dict(exp=0.69, r400=6.5, fhw=50, widen=1.0, budget=29, terr=0, tread=120, riserh=6, bperiod=9, bk=0.45, w_int=0.35),
 "River Valley": dict(exp=0.48, r400=2.2, fhw=165, widen=1.15, budget=42, terr=3, tread=120, riserh=8, bperiod=6, bk=0.2, w_int=0.35),
 "Hill Country": dict(exp=0.62, r400=51, fhw=26, widen=2.6, budget=85, terr=0, tread=120, riserh=11, bperiod=35, bk=0.6, w_int=0.35),
 "Heathland":    dict(exp=0.76, r400=1.7, fhw=235, widen=1.0, budget=19, terr=0, tread=120, riserh=0, bperiod=0, bk=0.0, w_int=0.35),
}

DOC = """<title>T1 · Macro profile explorer</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{--ground:#E7E9EC;--surface:#FFF;--surface2:#F1F4F7;--ink:#171A1F;--muted:#5A616C;--line:#CDD3DA;--accent:#2F7FC4;--r:3px}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){
 --ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38;--accent:#56B4FF}}
:root[data-theme="dark"]{--ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38;--accent:#56B4FF}
*{box-sizing:border-box}
body{margin:0;background:var(--ground);color:var(--ink);font:14px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
.wrap{max-width:1240px;margin:0 auto;padding:0 20px 60px}
header{padding:30px 0 16px;border-bottom:1px solid var(--line);margin-bottom:18px}
.eyebrow{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.16em;text-transform:uppercase;color:var(--muted);margin:0 0 8px}
h1{font-size:clamp(22px,2.6vw,32px);margin:0 0 8px;letter-spacing:-.02em}
.lede{margin:0;color:var(--muted);max-width:70ch}
.grid{display:grid;grid-template-columns:minmax(0,1fr) 300px;gap:18px}
@media (max-width:900px){.grid{grid-template-columns:1fr}}
canvas{width:100%;height:auto;display:block;border:1px solid var(--line);border-radius:var(--r);background:#000}
.panel{background:var(--surface);border:1px solid var(--line);border-radius:var(--r);padding:14px}
.panel h3{margin:0 0 8px;font-size:13px;text-transform:uppercase;letter-spacing:.08em;color:var(--muted)}
.presets{display:flex;flex-wrap:wrap;gap:6px;margin-bottom:12px}
.presets button{appearance:none;border:1px solid var(--line);background:var(--surface2);color:var(--ink);
 border-radius:99px;cursor:pointer;font:inherit;font-size:12px;padding:4px 11px}
.presets button:hover{border-color:var(--accent)}
.presets button.on{background:var(--ink);color:var(--surface);border-color:var(--ink)}
.ctl{margin-bottom:9px}
.ctl label{display:flex;justify-content:space-between;font-size:12.5px;color:var(--muted)}
.ctl label b{color:var(--ink);font-family:ui-monospace,Menlo,monospace;font-weight:600}
.ctl input[type=range]{width:100%;accent-color:var(--accent)}
.sect{border-top:1px solid var(--line);margin-top:10px;padding-top:10px}
svg{width:100%;height:150px;display:block;background:var(--surface);border:1px solid var(--line);border-radius:var(--r);margin-top:10px}
.note{font-size:12px;color:var(--muted);margin-top:10px}
.note code{font-family:ui-monospace,Menlo,monospace;background:var(--surface2);padding:1px 4px;border-radius:2px}
</style>
<div class="wrap">
<header>
<p class="eyebrow">Attempt 4 · T1 · design explorer</p>
<h1>Macro profile explorer — one real trunk, live parameters</h1>
<p class="lede">A real piedmont trunk (seed 2) with the T1 surface math mirrored in this page. Drag the dials across
their proposed record ranges; the terrain, the cross-section (white line on the map), and the slope readout update
live. Presets load each archetype's record midpoints. <strong>This is a design instrument, not the ruler</strong> —
the formulas mirror <code>macro_surface.rs</code> and simplify hardness to a constant.</p>
</header>
<div class="grid">
<div>
<canvas id="cv" width="470" height="470"></canvas>
<svg id="xs" viewBox="0 0 700 150" preserveAspectRatio="none"></svg>
<p class="note">Cross-section along the white transect (click the map to move it). Slope readout:
<span id="stats" style="font-family:ui-monospace,Menlo,monospace"></span></p>
</div>
<div class="panel">
<h3>Archetype presets</h3>
<div class="presets" id="presets"></div>
<h3>Catena (corpus-measured shape)</h3>
<div id="ctl-catena"></div>
<div class="sect"><h3>Terraces (river valley)</h3><div id="ctl-terr"></div></div>
<div class="sect"><h3>Benching (hc / plains)</h3><div id="ctl-bench"></div></div>
<div class="sect"><h3>Interfluve</h3><div id="ctl-int"></div></div>
</div>
</div>
</div>
<script>
const DATA = __DATA__;
const PRESETS = __PRESETS__;
const N=190, EXT=3000, CS=EXT/N;
const cv=document.getElementById('cv'), ctx=cv.getContext('2d');

// ---- precompute distance / arc / side ONCE (geometry is fixed)
let dist=new Float32Array(N*N), arcu=new Float32Array(N*N), sideA=new Float32Array(N*N);
(function(){
  // virtual extension, as in Rust
  const pts=DATA.pts.map(p=>({x:p[0],y:p[1]}));
  const z=DATA.z.slice();
  const ext=1600;
  {const d={x:pts[0].x-pts[1].x,y:pts[0].y-pts[1].y},l=Math.hypot(d.x,d.y);
   pts.unshift({x:pts[0].x+d.x/l*ext,y:pts[0].y+d.y/l*ext}); z.unshift(z[0]);}
  {const n=pts.length,d={x:pts[n-1].x-pts[n-2].x,y:pts[n-1].y-pts[n-2].y},l=Math.hypot(d.x,d.y);
   pts.push({x:pts[n-1].x+d.x/l*ext,y:pts[n-1].y+d.y/l*ext}); z.push(z[z.length-1]);}
  const cum=[0]; for(let i=1;i<pts.length;i++) cum.push(cum[i-1]+Math.hypot(pts[i].x-pts[i-1].x,pts[i].y-pts[i-1].y));
  window.ZBED=(a)=>{let lo=0,hi=cum.length-1;while(hi-lo>1){const m=(lo+hi)>>1;if(cum[m]<=a)lo=m;else hi=m;}
    const f=(a-cum[lo])/Math.max(1e-9,cum[lo+1]-cum[lo]);return z[lo]+(z[lo+1]-z[lo])*Math.min(1,Math.max(0,f));};
  for(let gy=0;gy<N;gy++)for(let gx=0;gx<N;gx++){
    const px=gx*CS, py=gy*CS; let bd=1e18,ba=0,bs=1;
    for(let i=0;i<pts.length-1;i++){
      const ax=pts[i].x,ay=pts[i].y,bx=pts[i+1].x,by=pts[i+1].y;
      const vx=bx-ax,vy=by-ay,L2=vx*vx+vy*vy;
      let t=L2>0?((px-ax)*vx+(py-ay)*vy)/L2:0; t=Math.min(1,Math.max(0,t));
      const qx=ax+vx*t,qy=ay+vy*t,d=Math.hypot(px-qx,py-qy);
      if(d<bd){bd=d;ba=cum[i]+Math.sqrt(L2)*t;bs=Math.sign(vx*(py-qy)-vy*(px-qx))||1;}
    }
    dist[gy*N+gx]=bd; arcu[gy*N+gx]=ba; sideA[gy*N+gx]=bs;
  }
  // 2x separable 5-tap gaussian on dist (the crease fix, mirrored)
  const K=[.0625,.25,.375,.25,.0625], tmp=new Float32Array(N*N);
  for(let p=0;p<2;p++){
    for(let y=0;y<N;y++)for(let x=0;x<N;x++){let a=0;for(let o=-2;o<=2;o++){const xx=Math.min(N-1,Math.max(0,x+o));a+=dist[y*N+xx]*K[o+2];}tmp[y*N+x]=a;}
    for(let y=0;y<N;y++)for(let x=0;x<N;x++){let a=0;for(let o=-2;o<=2;o++){const yy=Math.min(N-1,Math.max(0,y+o));a+=tmp[yy*N+x]*K[o+2];}dist[y*N+x]=a;}
  }
})();

const P={...PRESETS["Piedmont"]};
const CTLS=[
 ["ctl-catena","exp","catena exponent",0.40,1.10,0.01],
 ["ctl-catena","r400","rise @400 m (m)",1,65,0.5],
 ["ctl-catena","fhw","floor half-width (m)",15,300,1],
 ["ctl-catena","widen","floor widen ×",1,3.5,0.05],
 ["ctl-catena","budget","relief budget (m)",10,120,1],
 ["ctl-terr","terr","terrace steps",0,4,1],
 ["ctl-terr","tread","tread width (m)",60,200,2],
 ["ctl-terr","riserh","terrace riser (m)",2,20,0.5],
 ["ctl-bench","bperiod","bench period (m z)",0,60,1],
 ["ctl-bench","bk","bench strength",0,0.95,0.01],
 ["ctl-int","w_int","interfluve weight",0,0.6,0.01],
];
for(const [host,key,label,lo,hi,st] of CTLS){
  const d=document.createElement('div'); d.className='ctl';
  d.innerHTML=`<label>${label}<b id="v_${key}"></b></label><input id="s_${key}" type="range" min="${lo}" max="${hi}" step="${st}">`;
  document.getElementById(host).appendChild(d);
  const inp=d.querySelector('input'); inp.value=P[key];
  inp.addEventListener('input',()=>{P[key]=parseFloat(inp.value);render();});
}
const pr=document.getElementById('presets');
for(const name of Object.keys(PRESETS)){
  const b=document.createElement('button'); b.textContent=name;
  b.addEventListener('click',()=>{Object.assign(P,PRESETS[name]);
    for(const [,key] of CTLS.map(c=>[c[0],c[1]])) {const i=document.getElementById('s_'+key); if(i) i.value=P[key];}
    document.querySelectorAll('.presets button').forEach(x=>x.classList.toggle('on',x===b));
    render();});
  pr.appendChild(b);
}
pr.children[0].classList.add('on');

let transectY=1500;
cv.addEventListener('click',e=>{const r=cv.getBoundingClientRect();transectY=(1-(e.clientY-r.top)/r.height)*EXT;render();});

function relief(px,py){
  const n=DATA.rel_n, s=EXT/(n-1);
  let x=Math.min(n-2,Math.max(0,px/s)), y=Math.min(n-2,Math.max(0,py/s));
  const ix=Math.floor(x), iy=Math.floor(y), fx=x-ix, fy=y-iy;
  const g=(xx,yy)=>DATA.rel[yy*n+xx];
  return g(ix,iy)*(1-fx)*(1-fy)+g(ix+1,iy)*fx*(1-fy)+g(ix,iy+1)*(1-fx)*fy+g(ix+1,iy+1)*fx*fy;
}
function smoothstep(a,b,x){const t=Math.min(1,Math.max(0,(x-a)/(b-a)));return t*t*(3-2*t);}
function catena(u,r400,ce,fhw,cap){
  const den=Math.max(60,400-fhw); const r=r400*Math.pow(Math.max(0,u/den),ce);
  const k=Math.max(1,cap); const a=r/k; return k*a/(1+a)*(1+a/(1+a));
}
function heightAt(li,px,py){
  const d=dist[li], fhw=P.fhw*P.widen, cap=P.budget*0.8;
  const u=Math.max(0,d-fhw);
  let rise;
  const terr=P.terr>0 && sideA[li]>0;
  if(d<=fhw) rise=0;
  else if(terr){
    const k=Math.min(P.terr,Math.floor(u/P.tread)), f=Math.min(1,Math.max(0,u/P.tread-k));
    const soft=smoothstep(0.72,1.0,f); const stair=(k+soft)*P.riserh;
    rise = k>=P.terr ? stair+catena(u-P.terr*P.tread,P.r400,P.exp,fhw,cap) : stair;
  } else rise=catena(u,P.r400,P.exp,fhw,cap);
  let z=ZBED(arcu[li])+rise;
  const ramp=smoothstep(fhw+40,fhw+320,d);
  z+=P.w_int*P.budget*(relief(px,py)*0.5+0.5)*ramp;
  if(P.bk>0.05&&P.bperiod>0.5&&d>fhw){
    const cf=z/P.bperiod,k=Math.floor(cf),f=cf-k;
    const soft=smoothstep(0.72,0.97,f); const q=(k+soft)*P.bperiod;
    const w=Math.min(0.95,P.bk*1.25)*ramp; z=z*(1-w)+q*w;
  }
  return z;
}
const H=new Float32Array(N*N);
function render(){
  let lo=1e18,hi=-1e18;
  for(let gy=0;gy<N;gy++)for(let gx=0;gx<N;gx++){
    const z=heightAt(gy*N+gx,gx*CS,gy*CS); H[gy*N+gx]=z; if(z<lo)lo=z; if(z>hi)hi=z;
  }
  const span=Math.max(1e-9,hi-lo);
  const img=ctx.createImageData(470,470);
  const stops=[[0,[86,128,74]],[0.35,[168,158,96]],[0.7,[150,110,74]],[1,[235,228,214]]];
  const ramp=t=>{t=Math.min(1,Math.max(0,t));
    for(let i=0;i<3;i++){if(t<=stops[i+1][0]){const u=(t-stops[i][0])/(stops[i+1][0]-stops[i][0]);
      return stops[i][1].map((c,j)=>c+(stops[i+1][1][j]-c)*u);}} return stops[3][1];};
  const lz=Math.SQRT1_2, lxy=Math.SQRT1_2*Math.SQRT1_2;
  let cap8=0,cap15=0,tot=0;
  for(let py=0;py<470;py++)for(let px=0;px<470;px++){
    const wx=px/470*EXT, wy=(1-py/470)*EXT;
    const gx=Math.min(N-2,Math.max(1,wx/CS)), gy=Math.min(N-2,Math.max(1,wy/CS));
    const ix=Math.floor(gx), iy=Math.floor(gy);
    const h=(xx,yy)=>H[Math.min(N-1,yy)*N+Math.min(N-1,xx)];
    const zc=h(ix,iy);
    const dzx=(h(ix+1,iy)-h(ix-1,iy))/(2*CS), dzy=(h(ix,iy+1)-h(ix,iy-1))/(2*CS);
    const sl=Math.hypot(dzx,dzy);
    if(py%2===0&&px%2===0){tot++;if(sl<=0.08)cap8++;if(sl<=0.15)cap15++;}
    const L=Math.sqrt(dzx*dzx+dzy*dzy+1);
    const lam=Math.max(0,(-dzx*-lxy+ -dzy*lxy+1/L*lz)/1); // approx
    const shade=0.35+0.65*Math.max(0,(-dzx*(-lxy)-dzy*(lxy)+1/L*lz));
    const c=ramp((zc-lo)/span).map(v=>Math.min(255,v*shade));
    const o=(py*470+px)*4; img.data[o]=c[0];img.data[o+1]=c[1];img.data[o+2]=c[2];img.data[o+3]=255;
  }
  ctx.putImageData(img,0,0);
  // transect line
  ctx.strokeStyle='rgba(255,255,255,0.85)';ctx.lineWidth=1.2;
  const ty=(1-transectY/EXT)*470;ctx.beginPath();ctx.moveTo(0,ty);ctx.lineTo(470,ty);ctx.stroke();
  // cross-section
  const xs=document.getElementById('xs'); let path='',zlo=1e18,zhi=-1e18; const zs=[];
  for(let i=0;i<=350;i++){const wx=i/350*EXT;
    const gx=Math.min(N-1,wx/CS), gy=Math.min(N-1,transectY/CS);
    const z=H[Math.round(gy)*N+Math.round(gx)]; zs.push(z); if(z<zlo)zlo=z; if(z>zhi)zhi=z;}
  const zs2=Math.max(1e-9,zhi-zlo);
  zs.forEach((z,i)=>{path+=(i?'L':'M')+(i*2)+','+(140-130*(z-zlo)/zs2);});
  xs.innerHTML='<path d="'+path+'" fill="none" stroke="currentColor" stroke-width="1.6" opacity="0.9"/>'
    +'<text x="6" y="14" font-size="10" fill="currentColor" opacity="0.6">'+zhi.toFixed(0)+' m</text>'
    +'<text x="6" y="146" font-size="10" fill="currentColor" opacity="0.6">'+zlo.toFixed(0)+' m</text>';
  document.getElementById('stats').textContent=
    `relief ${(hi-lo).toFixed(1)} m · under 8% ${(100*cap8/tot).toFixed(1)}% · under 15% ${(100*cap15/tot).toFixed(1)}%`;
}
render();
</script>
"""
DOC = DOC.replace("__DATA__", DATA).replace("__PRESETS__", json.dumps(PRESETS))
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.2f} MB")
