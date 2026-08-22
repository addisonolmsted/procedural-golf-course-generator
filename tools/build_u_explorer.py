import json, pathlib
SP = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad")
DATA = (SP/"t1_data.json").read_text().strip()
OUT = SP/"u_explorer.html"

# presets mirror records.rs section tables: [kind, wLo, wHi, rLo, rHi, gate, hard]
PRESETS = {
 "Piedmont": dict(budget=50, fhw=32, widen=1.2, wvar=0.30, w_int=0.35, zones=[
   ["Slope",500,1100,12,26,1.0,0.0],["Bench",120,260,0,1.5,0.30,0.3],["Slope",300,700,6,14,0.8,0.0]]),
 "Great Plains": dict(budget=29, fhw=50, widen=1.0, wvar=0.25, w_int=0.35, zones=[
   ["Slope",140,300,2,5,1.0,0.0],["Scarp",30,70,3,8,0.75,0.6],["Bench",250,600,0,1,1.0,0.0]]),
 "River Valley": dict(budget=42, fhw=165, widen=1.15, wvar=0.40, w_int=0.35, zones=[
   ["Scarp",40,110,4,12,0.85,0.15],["Bench",150,400,0,2,0.9,0.0],["Scarp",40,120,4,14,0.6,0.15],
   ["Bench",150,380,0,2,0.6,0.0],["Scarp",50,130,5,16,0.45,0.15],["Slope",250,600,4,10,0.8,0.0]]),
 "Hill Country": dict(budget=85, fhw=26, widen=2.6, wvar=0.35, w_int=0.35, zones=[
   ["Slope",150,320,8,18,1.0,0.0],["Scarp",25,60,12,26,0.8,0.8],["Bench",130,300,0,2,0.8,0.4],
   ["Scarp",25,70,8,20,0.6,0.8],["Slope",200,450,8,18,0.9,0.0]]),
 "Heathland": dict(budget=19, fhw=235, widen=1.0, wvar=0.30, w_int=0.35, zones=[
   ["Slope",400,900,2,6,1.0,0.0]]),
}

DOC = """<title>U · Section explorer</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{--ground:#E7E9EC;--surface:#FFF;--surface2:#F1F4F7;--ink:#171A1F;--muted:#5A616C;--line:#CDD3DA;--accent:#2F7FC4;--r:3px}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){
 --ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38;--accent:#56B4FF}}
:root[data-theme="dark"]{--ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38;--accent:#56B4FF}
*{box-sizing:border-box}
body{margin:0;background:var(--ground);color:var(--ink);font:14px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
.wrap{max-width:1280px;margin:0 auto;padding:0 20px 60px}
header{padding:28px 0 14px;border-bottom:1px solid var(--line);margin-bottom:16px}
.eyebrow{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.16em;text-transform:uppercase;color:var(--muted);margin:0 0 8px}
h1{font-size:clamp(21px,2.5vw,30px);margin:0 0 8px;letter-spacing:-.02em}
.lede{margin:0;color:var(--muted);max-width:74ch}
.grid{display:grid;grid-template-columns:minmax(0,1fr) 360px;gap:16px}
@media (max-width:960px){.grid{grid-template-columns:1fr}}
canvas{width:100%;height:auto;display:block;border:1px solid var(--line);border-radius:var(--r);background:#000}
.panel{background:var(--surface);border:1px solid var(--line);border-radius:var(--r);padding:12px;max-height:760px;overflow-y:auto}
.panel h3{margin:10px 0 6px;font-size:12px;text-transform:uppercase;letter-spacing:.08em;color:var(--muted)}
.panel h3:first-child{margin-top:0}
.presets{display:flex;flex-wrap:wrap;gap:6px;margin-bottom:8px}
.presets button{appearance:none;border:1px solid var(--line);background:var(--surface2);color:var(--ink);border-radius:99px;cursor:pointer;font:inherit;font-size:12px;padding:3px 10px}
.presets button.on{background:var(--ink);color:var(--surface);border-color:var(--ink)}
.ctl{margin-bottom:7px}
.ctl label{display:flex;justify-content:space-between;font-size:12px;color:var(--muted)}
.ctl label b{color:var(--ink);font-family:ui-monospace,Menlo,monospace;font-weight:600}
.ctl input[type=range]{width:100%;accent-color:var(--accent)}
.zone{border:1px solid var(--line-soft,var(--line));border-radius:var(--r);padding:8px;margin-bottom:8px;background:var(--surface2)}
.zone .zh{display:flex;justify-content:space-between;align-items:center;margin-bottom:5px}
.zone .zh b{font-size:12px}
.zone .zh .kind{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:var(--accent)}
svg{width:100%;height:150px;display:block;background:var(--surface);border:1px solid var(--line);border-radius:var(--r);margin-top:10px}
.note{font-size:12px;color:var(--muted);margin-top:8px}
.side-toggle{display:flex;gap:6px;margin-bottom:8px}
.side-toggle button{flex:1;appearance:none;border:1px solid var(--line);background:var(--surface2);color:var(--muted);border-radius:var(--r);cursor:pointer;font:inherit;font-size:12px;padding:4px}
.side-toggle button.on{background:var(--accent);color:#fff;border-color:var(--accent)}
</style>
<div class="wrap">
<header>
<p class="eyebrow">Attempt 4 · U-series · section explorer</p>
<h1>The section program, live — zones per side on a real valley axis</h1>
<p class="lede">A real trunk at the new valley-axis scale. Edit the zone program (widths, rises, gates) per SIDE, or
edit both together; the terrain, cross-section and slope readout update live. Gates &lt; 1 make zones come and go
along the trunk — that is the terrace-variety mechanism. Values you settle on go into the records as golf-provenance
ranges. Design instrument, not the ruler.</p>
</header>
<div class="grid">
<div>
<canvas id="cv" width="470" height="470"></canvas>
<svg id="xs" viewBox="0 0 700 150" preserveAspectRatio="none"></svg>
<p class="note">Cross-section at the white transect (click map to move). <span id="stats" style="font-family:ui-monospace,Menlo,monospace"></span></p>
</div>
<div class="panel">
<h3>Presets</h3><div class="presets" id="presets"></div>
<h3>Editing side</h3>
<div class="side-toggle"><button id="sideA" class="on">Left</button><button id="sideB">Right</button><button id="sideBoth">Both</button></div>
<h3>Global</h3><div id="ctl-global"></div>
<h3>Zone program (floor → crown)</h3><div id="zones"></div>
</div>
</div>
</div>
<script>
const DATA=__DATA__, PRESETS=__PRESETS__;
const N=190, EXT=3000, CS=EXT/N;
const cv=document.getElementById('cv'), ctx=cv.getContext('2d');

// geometry precompute (extension + arc + signed lateral + 2x gaussian on dist)
let dist=new Float32Array(N*N), arcu=new Float32Array(N*N), lat=new Float32Array(N*N);
let TOTAL_ARC=0;
(function(){
  const pts=DATA.pts.map(p=>({x:p[0],y:p[1]})), z=DATA.z.slice(), ext=1600;
  {const d={x:pts[0].x-pts[1].x,y:pts[0].y-pts[1].y},l=Math.hypot(d.x,d.y);
   pts.unshift({x:pts[0].x+d.x/l*ext,y:pts[0].y+d.y/l*ext}); z.unshift(z[0]);}
  {const n=pts.length,d={x:pts[n-1].x-pts[n-2].x,y:pts[n-1].y-pts[n-2].y},l=Math.hypot(d.x,d.y);
   pts.push({x:pts[n-1].x+d.x/l*ext,y:pts[n-1].y+d.y/l*ext}); z.push(z[z.length-1]);}
  const cum=[0]; for(let i=1;i<pts.length;i++) cum.push(cum[i-1]+Math.hypot(pts[i].x-pts[i-1].x,pts[i].y-pts[i-1].y));
  TOTAL_ARC=cum[cum.length-1];
  window.ZBED=a=>{let lo=0,hi=cum.length-1;while(hi-lo>1){const m=(lo+hi)>>1;if(cum[m]<=a)lo=m;else hi=m;}
    const f=(a-cum[lo])/Math.max(1e-9,cum[lo+1]-cum[lo]);return z[lo]+(z[lo+1]-z[lo])*Math.min(1,Math.max(0,f));};
  for(let gy=0;gy<N;gy++)for(let gx=0;gx<N;gx++){
    const px=gx*CS,py=gy*CS;let bd=1e18,ba=0,bs=1;
    for(let i=0;i<pts.length-1;i++){
      const ax=pts[i].x,ay=pts[i].y,vx=pts[i+1].x-ax,vy=pts[i+1].y-ay,L2=vx*vx+vy*vy;
      let t=L2>0?((px-ax)*vx+(py-ay)*vy)/L2:0;t=Math.min(1,Math.max(0,t));
      const qx=ax+vx*t,qy=ay+vy*t,d=Math.hypot(px-qx,py-qy);
      if(d<bd){bd=d;ba=cum[i]+Math.sqrt(L2)*t;bs=Math.sign(vx*(py-qy)-vy*(px-qx))||1;}
    }
    dist[gy*N+gx]=bd;arcu[gy*N+gx]=ba;lat[gy*N+gx]=bs*bd;
  }
  const K=[.0625,.25,.375,.25,.0625],tmp=new Float32Array(N*N);
  for(let p=0;p<2;p++){
    for(let y=0;y<N;y++)for(let x=0;x<N;x++){let a=0;for(let o=-2;o<=2;o++){const xx=Math.min(N-1,Math.max(0,x+o));a+=dist[y*N+xx]*K[o+2];}tmp[y*N+x]=a;}
    for(let y=0;y<N;y++)for(let x=0;x<N;x++){let a=0;for(let o=-2;o<=2;o++){const yy=Math.min(N-1,Math.max(0,y+o));a+=tmp[yy*N+x]*K[o+2];}dist[y*N+x]=a;}
  }
})();

// deterministic 1D value noise
function vnoise(x,seed){
  const h=i=>{let v=(i*374761393+seed*668265263)>>>0;v=(v^(v>>13))>>>0;v=(v*1274126177)>>>0;return ((v^(v>>16))>>>0)/4294967295*2-1;};
  const i=Math.floor(x),f=x-i,u=f*f*(3-2*f);
  return h(i)*(1-u)+h(i+1)*u;
}
function sstep(a,b,x){const t=Math.min(1,Math.max(0,(x-a)/(b-a)));return t*t*(3-2*t);}
function relief(px,py){
  const n=DATA.rel_n,s=EXT/(n-1);
  let x=Math.min(n-2,Math.max(0,px/s)),y=Math.min(n-2,Math.max(0,py/s));
  const ix=Math.floor(x),iy=Math.floor(y),fx=x-ix,fy=y-iy,g=(xx,yy)=>DATA.rel[yy*n+xx];
  return g(ix,iy)*(1-fx)*(1-fy)+g(ix+1,iy)*fx*(1-fy)+g(ix,iy+1)*(1-fx)*fy+g(ix+1,iy+1)*fx*fy;
}

let G={budget:50,fhw:32,widen:1.2,wvar:0.3,w_int:0.35};
let ZONES=[[],[]]; // per side: [kind,w,r,gate,hard,seed]
let editSide=2; // 0,1,2=both

function loadPreset(p){
  G={budget:p.budget,fhw:p.fhw,widen:p.widen,wvar:p.wvar,w_int:p.w_int};
  ZONES=[0,1].map(si=>p.zones.map((z,zi)=>({kind:z[0],w:(z[1]+z[2])/2,r:(z[3]+z[4])/2,gate:z[5],hard:z[6],seed:100+si*37+zi*11})));
  buildUI();render();
}
const STATION=150;
function sideRise(si,arc,d){
  const zs=ZONES[si];
  const fhw=G.fhw*G.widen*(1+G.wvar*vnoise(arc/1500,7+si*13));
  let u=d-Math.max(8,fhw);
  if(u<=0)return 0;
  const fade=Math.min(1,Math.max(0,1-(d-350)/650));
  let acc=0;
  for(const z of zs){
    const gn=vnoise(arc/900,z.seed)*0.5+0.5;
    const thr=1-z.gate;
    const gArc=sstep(thr-0.12,thr+0.12,gn);
    const g=z.gate*(1-fade)+gArc*fade;  // mean approximated by gate_p
    if(g<=0.01)continue;
    const wmArc=1+G.wvar*vnoise(arc/1300,z.seed+5);
    const wm=1*(1-fade)+wmArc*fade;
    const zw=Math.max(4,z.w*wm*g), zr=z.r*g;
    if(u<=zw){
      const f=u/zw;
      const shape=z.kind==='Slope'?sstep(0,1,f):z.kind==='Scarp'?sstep(0.36,0.64,f):f;
      return acc+zr*shape;
    }
    acc+=zr;u-=zw;
  }
  return acc+u*0.015;
}
function heightAt(li,px,py){
  const d=dist[li],a=arcu[li];
  const sw=sstep(-140,140,lat[li]);
  let raw=sw<0.02?sideRise(0,a,d):sw>0.98?sideRise(1,a,d):sideRise(0,a,d)*(1-sw)+sideRise(1,a,d)*sw;
  const k=Math.max(1,G.budget*0.8),aa=raw/k;
  const rise=k*aa/(1+aa)*(1+aa/(1+aa));
  let z=ZBED(a)+rise;
  const fhw=G.fhw*G.widen;
  z+=G.w_int*G.budget*(relief(px,py)*0.5+0.5)*sstep(fhw+40,fhw+320,d);
  return z;
}
const H=new Float32Array(N*N);
let transectY=1500;
cv.addEventListener('click',e=>{const r=cv.getBoundingClientRect();transectY=(1-(e.clientY-r.top)/r.height)*EXT;render();});
function render(){
  let lo=1e18,hi=-1e18;
  for(let gy=0;gy<N;gy++)for(let gx=0;gx<N;gx++){const z=heightAt(gy*N+gx,gx*CS,gy*CS);H[gy*N+gx]=z;if(z<lo)lo=z;if(z>hi)hi=z;}
  const span=Math.max(1e-9,hi-lo),img=ctx.createImageData(470,470);
  const stops=[[0,[86,128,74]],[0.35,[168,158,96]],[0.7,[150,110,74]],[1,[235,228,214]]];
  const ramp=t=>{t=Math.min(1,Math.max(0,t));for(let i=0;i<3;i++){if(t<=stops[i+1][0]){const u=(t-stops[i][0])/(stops[i+1][0]-stops[i][0]);
    return stops[i][1].map((c,j)=>c+(stops[i+1][1][j]-c)*u);}}return stops[3][1];};
  let cap8=0,cap15=0,tot=0;
  const lxy=0.5,lz=Math.SQRT1_2;
  for(let py=0;py<470;py++)for(let px=0;px<470;px++){
    const wx=px/470*EXT,wy=(1-py/470)*EXT;
    const gx=Math.min(N-2,Math.max(1,wx/CS)),gy=Math.min(N-2,Math.max(1,wy/CS));
    const ix=Math.floor(gx),iy=Math.floor(gy);
    const h=(xx,yy)=>H[Math.min(N-1,Math.max(0,yy))*N+Math.min(N-1,Math.max(0,xx))];
    const zc=h(ix,iy);
    const dzx=(h(ix+1,iy)-h(ix-1,iy))/(2*CS),dzy=(h(ix,iy+1)-h(ix,iy-1))/(2*CS);
    if(py%2===0&&px%2===0){tot++;const sl=Math.hypot(dzx,dzy);if(sl<=0.08)cap8++;if(sl<=0.15)cap15++;}
    const L=Math.sqrt(dzx*dzx+dzy*dzy+1);
    const shade=0.35+0.65*Math.max(0,(dzx*lxy - dzy*lxy + lz)/L);
    const c=ramp((zc-lo)/span).map(v=>Math.min(255,v*shade));
    const o=(py*470+px)*4;img.data[o]=c[0];img.data[o+1]=c[1];img.data[o+2]=c[2];img.data[o+3]=255;
  }
  ctx.putImageData(img,0,0);
  ctx.strokeStyle='rgba(255,255,255,0.85)';ctx.lineWidth=1.2;
  const ty=(1-transectY/EXT)*470;ctx.beginPath();ctx.moveTo(0,ty);ctx.lineTo(470,ty);ctx.stroke();
  const xs=document.getElementById('xs');let path='',zlo=1e18,zhi=-1e18;const zs=[];
  for(let i=0;i<=350;i++){const wx=i/350*EXT;
    const gy2=Math.min(N-1,Math.round(transectY/CS)),gx2=Math.min(N-1,Math.round(wx/CS));
    const z=H[gy2*N+gx2];zs.push(z);if(z<zlo)zlo=z;if(z>zhi)zhi=z;}
  const zs2=Math.max(1e-9,zhi-zlo);
  zs.forEach((z,i)=>{path+=(i?'L':'M')+(i*2)+','+(140-130*(z-zlo)/zs2);});
  xs.innerHTML='<path d="'+path+'" fill="none" stroke="currentColor" stroke-width="1.6" opacity="0.9"/>'
   +'<text x="6" y="14" font-size="10" fill="currentColor" opacity="0.6">'+zhi.toFixed(0)+' m</text>'
   +'<text x="6" y="146" font-size="10" fill="currentColor" opacity="0.6">'+zlo.toFixed(0)+' m</text>';
  document.getElementById('stats').textContent=
    `relief ${(hi-lo).toFixed(1)} m · under 8% ${(100*cap8/tot).toFixed(1)}% · under 15% ${(100*cap15/tot).toFixed(1)}%`;
}

function slider(host,label,val,lo,hi,st,cb){
  const d=document.createElement('div');d.className='ctl';
  d.innerHTML=`<label>${label}<b></b></label><input type="range" min="${lo}" max="${hi}" step="${st}">`;
  const inp=d.querySelector('input'),bb=d.querySelector('b');
  inp.value=val;bb.textContent=(+val).toFixed(st<1?2:0);
  inp.addEventListener('input',()=>{bb.textContent=(+inp.value).toFixed(st<1?2:0);cb(parseFloat(inp.value));render();});
  host.appendChild(d);return inp;
}
function buildUI(){
  const g=document.getElementById('ctl-global');g.innerHTML='';
  slider(g,'relief budget (m)',G.budget,10,120,1,v=>G.budget=v);
  slider(g,'floor half-width (m)',G.fhw,15,300,1,v=>G.fhw=v);
  slider(g,'floor widen ×',G.widen,1,3.5,0.05,v=>G.widen=v);
  slider(g,'width variation ±',G.wvar,0,0.6,0.01,v=>G.wvar=v);
  slider(g,'interfluve weight',G.w_int,0,0.6,0.01,v=>G.w_int=v);
  const zh=document.getElementById('zones');zh.innerHTML='';
  const si=editSide===2?0:editSide;
  ZONES[si].forEach((z,zi)=>{
    const box=document.createElement('div');box.className='zone';
    box.innerHTML=`<div class="zh"><b>zone ${zi+1}</b><span class="kind">${z.kind}</span></div>`;
    const set=(k,v)=>{if(editSide===2){ZONES[0][zi][k]=v;ZONES[1][zi][k]=v;}else ZONES[editSide][zi][k]=v;};
    slider(box,'width (m)',z.w,10,900,5,v=>set('w',v));
    slider(box,'rise (m)',z.r,0,30,0.5,v=>set('r',v));
    slider(box,'gate (share of trunk)',z.gate,0,1,0.01,v=>set('gate',v));
    slider(box,'hardness gate',z.hard,0,1,0.01,v=>set('hard',v));
    zh.appendChild(box);
  });
}
const pr=document.getElementById('presets');
for(const name of Object.keys(PRESETS)){
  const b=document.createElement('button');b.textContent=name;
  b.addEventListener('click',()=>{document.querySelectorAll('.presets button').forEach(x=>x.classList.toggle('on',x===b));loadPreset(PRESETS[name]);});
  pr.appendChild(b);
}
for(const [id,val] of [['sideA',0],['sideB',1],['sideBoth',2]]){
  document.getElementById(id).addEventListener('click',()=>{
    editSide=val;
    document.querySelectorAll('.side-toggle button').forEach(x=>x.classList.remove('on'));
    document.getElementById(id).classList.add('on');
    buildUI();
  });
}
document.getElementById('sideBoth').classList.add('on');document.getElementById('sideA').classList.remove('on');
loadPreset(PRESETS['River Valley']);
pr.children[2].classList.add('on');
</script>
"""
DOC = DOC.replace("__DATA__", DATA).replace("__PRESETS__", json.dumps(PRESETS))
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.2f} MB")
