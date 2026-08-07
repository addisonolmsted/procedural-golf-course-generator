"""Emit the self-contained Parkland Atlas HTML viewer.

Faithful to the reference ``parkland_atlas.html`` (same decode / hillshade /
hole / profile code) with two additions the task asked for:

  * a **terrain** layer toggle (the hillshade base becomes togglable — off
    shows a dark ground so tree/water/holes read on their own),
  * a **holes** layer toggle,

plus a rail that scales to the full 200-course set, grouped lowland / rolling /
mountain by relief.
"""

import json

_HEAD = """<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Parkland Atlas — Terrain, Water, Canopy &amp; Routing</title>
<style>
  :root{--paper:#0C1310;--panel:#101A15;--line:#22352B;--ink:#E6E0CC;
    --dim:#8CA093;--accent:#E4573D;--fair:#B7CE6A;--water:#5E93B8;--tree:#6FA860;}
  *{box-sizing:border-box;margin:0;padding:0}
  html,body{background:var(--paper);color:var(--ink);
    font-family:Georgia,'Times New Roman',serif;font-size:15px;line-height:1.45}
  .frame{max-width:1180px;margin:0 auto;padding:26px 20px 60px}
  header{border-bottom:1px solid var(--line);padding-bottom:14px;margin-bottom:18px;
    display:flex;justify-content:space-between;align-items:baseline;flex-wrap:wrap;gap:8px}
  h1{font-size:21px;font-weight:normal;letter-spacing:.06em}
  h1 .no{color:var(--accent);font-family:ui-monospace,Menlo,monospace;font-size:13px;margin-right:10px}
  .sub{color:var(--dim);font-size:12.5px;font-style:italic}
  .layout{display:grid;grid-template-columns:230px 1fr;gap:18px}
  @media(max-width:860px){.layout{grid-template-columns:1fr}}
  .rail{display:flex;flex-direction:column;gap:5px;max-height:1560px;overflow-y:auto;padding-right:4px}
  .ghdr{color:var(--dim);font-size:9.5px;letter-spacing:.16em;text-transform:uppercase;
    font-family:ui-monospace,Menlo,monospace;margin:10px 0 2px;border-bottom:1px solid var(--line);padding-bottom:3px}
  .ghdr:first-child{margin-top:0}
  .card{border:1px solid var(--line);background:var(--panel);padding:7px 10px;cursor:pointer;transition:border-color .15s}
  .card:hover{border-color:var(--dim)}
  .card.on{border-color:var(--accent)}
  .card .nm{font-size:13.5px}
  .card .ar{color:var(--dim);font-size:11px;font-style:italic;margin-top:2px}
  .card .rl{font-family:ui-monospace,Menlo,monospace;font-size:10.5px;color:var(--fair);margin-top:3px}
  .sheet{border:1px solid var(--line);background:var(--panel);padding:16px}
  .sheet h2{font-size:17px;font-weight:normal;margin-bottom:2px}
  .sheet .arch{color:var(--dim);font-size:12px;font-style:italic;margin-bottom:12px}
  .stats{display:flex;flex-wrap:wrap;gap:0;border:1px solid var(--line);margin-bottom:14px}
  .stat{flex:1 1 110px;padding:8px 12px;border-right:1px solid var(--line)}
  .stat:last-child{border-right:0}
  .stat .k{color:var(--dim);font-size:10px;letter-spacing:.14em;text-transform:uppercase}
  .stat .v{font-family:ui-monospace,Menlo,monospace;font-size:15px;margin-top:2px}
  .stat .v small{color:var(--dim);font-size:10px}
  canvas{display:block;width:100%;height:auto;background:#0A100D}
  .layerbar{display:flex;align-items:baseline;gap:10px;margin-bottom:6px;flex-wrap:wrap}
  .lchip{font-family:ui-monospace,Menlo,monospace;font-size:10.5px;color:var(--dim);
    border:1px solid var(--line);padding:3px 9px;cursor:pointer;user-select:none}
  .lchip.on{color:var(--fair);border-color:var(--fair)}
  .lchip#tgWater.on{color:var(--water);border-color:var(--water)}
  .lchip#tgTree.on{color:var(--tree);border-color:var(--tree)}
  .lchip#tgTerrain.on{color:var(--ink);border-color:var(--ink)}
  .lsrc{color:var(--dim);font-size:10px;font-style:italic;margin-left:auto}
  .mapwrap{position:relative;border:1px solid var(--line)}
  .caption{color:var(--dim);font-size:11px;font-style:italic;margin:8px 2px 16px}
  .profcap{display:flex;justify-content:space-between;align-items:baseline;margin:2px 2px 6px}
  .profcap .t{font-size:13px;letter-spacing:.05em}
  .readout{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:var(--fair);min-height:15px}
  footer{color:var(--dim);font-size:11px;font-style:italic;border-top:1px solid var(--line);
    margin-top:26px;padding-top:10px;line-height:1.7}
</style></head>
<body><div class="frame">
  <header>
    <h1><span class="no">SURVEY III</span>Parkland Atlas — Terrain, Water, Canopy &amp; Routing</h1>
    <div class="sub" id="subcount"></div>
  </header>
  <div class="layout">
    <nav class="rail" id="rail"></nav>
    <main class="sheet">
      <h2 id="cname"></h2>
      <div class="arch" id="carch"></div>
      <div class="stats" id="stats"></div>
      <div class="layerbar">
        <span class="lchip on" id="tgTerrain">▬ terrain</span>
        <span class="lchip on" id="tgTree">⬤ tree canopy</span>
        <span class="lchip on" id="tgWater">⬤ water</span>
        <span class="lchip on" id="tgHoles">╱ holes</span>
        <span class="lsrc">Mapzen/AWS terrain · ESA WorldCover 10 m · OSM hydrography + routing</span>
      </div>
      <div class="mapwrap"><canvas id="map"></canvas></div>
      <div class="caption" id="mapcap"></div>
      <div class="profcap">
        <div class="t">ELEVATION ALONG THE LINE OF PLAY</div>
        <div class="readout" id="readout">&nbsp;</div>
      </div>
      <canvas id="prof" height="240"></canvas>
      <div class="caption">Hover the profile to place the point on the map. Bands alternate per hole; ▼▲ mark the property low/high.</div>
    </main>
  </div>
  <footer>
    Hole routings © OpenStreetMap contributors (ODbL). Elevation from AWS/Mapzen Terrain Tiles
    (USGS 3DEP / SRTM source) sampled at zoom 15. Tree canopy from ESA WorldCover 2021 v200 (10 m, © ESA, CC BY 4.0);
    water from ESA WorldCover water class unioned with OSM hydrography polygons. Coverage percentages are of the survey extent.
    Multi-course clubs split to a single routing by tee-to-green continuity. <span id="datestamp"></span>
  </footer>
</div>
<script>
window.COURSES=__COURSES__;
</script>
<script>
window.GROUPS=__GROUPS__;
</script>
<script>
"use strict";
const $=q=>document.querySelector(q);
const N=256;
const GROUPS=window.GROUPS;
const ORDER=GROUPS.flatMap(g=>g.keys);
"""

_BODY = r"""
function decodeElev(c){const bin=atob(c.b64),g=new Float32Array(N*N);
  for(let i=0;i<N*N;i++)g[i]=c.emin+((bin.charCodeAt(2*i)|(bin.charCodeAt(2*i+1)<<8)))/10;return g;}
function decodeMask(b64){const bin=atob(b64),m=new Uint8Array(N*N);
  for(let i=0;i<bin.length;i++){const b=bin.charCodeAt(i);for(let k=0;k<8;k++)m[i*8+k]=(b>>(7-k))&1;}return m;}
function maskCanvas(m,r,g,b,a){const cv=document.createElement('canvas');cv.width=N;cv.height=N;
  const img=new ImageData(N,N);
  for(let i=0;i<N*N;i++){if(m[i]){img.data[4*i]=r;img.data[4*i+1]=g;img.data[4*i+2]=b;img.data[4*i+3]=a;}}
  cv.getContext('2d').putImageData(img,0,0);return cv;}
const CACHE={};
function course(k){if(!CACHE[k]){const c=COURSES[k];
  CACHE[k]={...c,grid:decodeElev(c),tree:decodeMask(c.tb64),water:decodeMask(c.wb64)};}return CACHE[k];}
function ll2xy(c,lat,lon,W,H){const[la0,lo0,la1,lo1]=c.bbox;
  return{x:(lon-lo0)/(lo1-lo0)*W,y:(la1-lat)/(la1-la0)*H};}
function holeCum(c,hi){const pts=c.holes[hi].pts,latc=(c.bbox[0]+c.bbox[2])/2,k=Math.cos(latc*Math.PI/180);
  const cum=[0];for(let i=1;i<pts.length;i++){const dy=(pts[i][0]-pts[i-1][0])*111320,dx=(pts[i][1]-pts[i-1][1])*111320*k;
  cum.push(cum[cum.length-1]+Math.hypot(dx,dy));}return cum;}
function pointAlong(c,hi,t){const pts=c.holes[hi].pts,cum=holeCum(c,hi);const L=cum[cum.length-1];t=Math.max(0,Math.min(L,t));
  let j=0;while(j<cum.length-2&&cum[j+1]<t)j++;const f=(t-cum[j])/Math.max(1e-9,cum[j+1]-cum[j]);
  return[pts[j][0]+(pts[j+1][0]-pts[j][0])*f,pts[j][1]+(pts[j+1][1]-pts[j][1])*f];}
const RAMP=[[0,[26,46,36]],[.18,[38,66,44]],[.38,[74,96,52]],[.58,[122,120,66]],[.76,[168,146,92]],[.9,[204,180,132]],[1,[232,214,178]]];
function rampCol(t){for(let i=1;i<RAMP.length;i++){if(t<=RAMP[i][0]){const[a,ca]=RAMP[i-1],[b,cb]=RAMP[i],f=(t-a)/(b-a);
  return[ca[0]+(cb[0]-ca[0])*f,ca[1]+(cb[1]-ca[1])*f,ca[2]+(cb[2]-ca[2])*f];}}return RAMP[RAMP.length-1][1];}
function renderTerrain(c){const g=c.grid,cw=c.wm/N,ch=c.hm/N,img=new ImageData(N,N);
  const az=315*Math.PI/180,alt=45*Math.PI/180,lx=Math.cos(alt)*Math.cos(az),ly=Math.cos(alt)*Math.sin(az),lz=Math.sin(alt);
  const rng=Math.max(1e-6,c.emax-c.emin);
  for(let y=0;y<N;y++)for(let x=0;x<N;x++){const i=y*N+x;
    const xm=x>0?x-1:x,xp=x<N-1?x+1:x,ym=y>0?y-1:y,yp=y<N-1?y+1:y;
    const dzdx=(g[y*N+xp]-g[y*N+xm])/((xp-xm)*cw),dzdy=(g[yp*N+x]-g[ym*N+x])/((yp-ym)*ch);
    let nx=-dzdx,ny=dzdy,nz=1;const nl=Math.hypot(nx,ny,nz);nx/=nl;ny/=nl;nz/=nl;
    const sh=Math.max(0,nx*lx+ny*ly+nz*lz),t=(g[i]-c.emin)/rng,col=rampCol(t),lum=.35+.75*sh;
    img.data[4*i]=col[0]*lum;img.data[4*i+1]=col[1]*lum;img.data[4*i+2]=col[2]*lum;img.data[4*i+3]=255;}
  return img;}
const HOLECOL=["#E8D68F","#D9A46B","#C9E0A2","#9FCB9B","#8FBFB4","#88AECC","#A695C6","#C58FB4","#D98D86",
  "#E8B07F","#EBD37E","#B9D77E","#93C79C","#84B9B9","#8AA3CE","#B79BCB","#D093B5","#DFA08A"];
let cur=null,hoverPt=null;
const LAYERS={terrain:true,tree:true,water:true,holes:true};
function drawMap(){const c=cur,cv=$('#map');const cssW=cv.clientWidth||900,W=Math.min(1400,cssW*1.6),H=W*c.hm/c.wm;
  cv.width=W;cv.height=H;const ctx=cv.getContext('2d');
  if(LAYERS.terrain){const off=document.createElement('canvas');off.width=N;off.height=N;
    off.getContext('2d').putImageData(c.timg,0,0);ctx.imageSmoothingEnabled=true;ctx.imageSmoothingQuality='high';
    ctx.drawImage(off,0,0,W,H);}else{ctx.fillStyle='#0A100D';ctx.fillRect(0,0,W,H);}
  if(LAYERS.tree){if(!c.treeCv)c.treeCv=maskCanvas(c.tree,20,52,30,150);ctx.drawImage(c.treeCv,0,0,W,H);}
  if(LAYERS.water){if(!c.waterCv)c.waterCv=maskCanvas(c.water,70,128,176,205);ctx.drawImage(c.waterCv,0,0,W,H);}
  const P=(lat,lon)=>ll2xy(c,lat,lon,W,H);
  if(LAYERS.holes)c.holes.forEach((h,i)=>{const pts=h.pts.map(p=>P(p[0],p[1]));
    ctx.lineJoin='round';ctx.lineCap='round';ctx.strokeStyle='rgba(12,19,16,0.55)';ctx.lineWidth=4.5;
    ctx.beginPath();pts.forEach((p,k)=>k?ctx.lineTo(p.x,p.y):ctx.moveTo(p.x,p.y));ctx.stroke();
    ctx.strokeStyle=HOLECOL[i%18];ctx.lineWidth=1.8;ctx.setLineDash([7,4]);
    ctx.beginPath();pts.forEach((p,k)=>k?ctx.lineTo(p.x,p.y):ctx.moveTo(p.x,p.y));ctx.stroke();ctx.setLineDash([]);
    const tp=pts[0],gp=pts[pts.length-1];ctx.fillStyle='#E6E0CC';ctx.fillRect(tp.x-3,tp.y-3,6,6);
    ctx.beginPath();ctx.arc(gp.x,gp.y,3.6,0,7);ctx.fillStyle='#0C1310';ctx.fill();ctx.strokeStyle='#E4573D';ctx.lineWidth=1.8;ctx.stroke();
    const mid=pts[(pts.length/2)|0];ctx.fillStyle='rgba(12,19,16,0.8)';ctx.beginPath();ctx.arc(mid.x,mid.y,8,0,7);ctx.fill();
    ctx.fillStyle=HOLECOL[i%18];ctx.font='bold 10px ui-monospace,Menlo,monospace';ctx.textAlign='center';
    ctx.fillText(String(h.ref),mid.x,mid.y+3.5);ctx.textAlign='left';});
  const mk=(m,sym,col)=>{const x=(m.gx+0.5)/N*W,y=(m.gy+0.5)/N*H;
    ctx.font='bold 13px ui-monospace,Menlo,monospace';ctx.fillStyle=col;ctx.textAlign='center';ctx.fillText(sym,x,y+4);
    ctx.font='10px ui-monospace,Menlo,monospace';ctx.fillStyle='rgba(12,19,16,0.75)';const lbl=m.v.toFixed(0)+' m';
    const tw=ctx.measureText(lbl).width;ctx.fillRect(x-tw/2-3,y+7,tw+6,13);ctx.fillStyle=col;ctx.fillText(lbl,x,y+17);ctx.textAlign='left';};
  if(c.min&&c.max){mk(c.min,'▼',"#7FB7DE");mk(c.max,'▲',"#E4573D");}
  const mpp=c.wm/W,bar=Math.pow(10,Math.floor(Math.log10(W*0.22*mpp))),barpx=bar/mpp;
  ctx.strokeStyle='#E6E0CC';ctx.lineWidth=1.4;ctx.beginPath();ctx.moveTo(16,H-16);ctx.lineTo(16+barpx,H-16);
  ctx.moveTo(16,H-20);ctx.lineTo(16,H-12);ctx.moveTo(16+barpx,H-20);ctx.lineTo(16+barpx,H-12);ctx.stroke();
  ctx.font='10px ui-monospace,Menlo,monospace';ctx.fillStyle='#E6E0CC';ctx.fillText(bar>=1000?(bar/1000)+' km':bar+' m',16,H-24);ctx.fillText('N ↑',W-34,22);
  if(hoverPt){const p=P(hoverPt[0],hoverPt[1]);ctx.beginPath();ctx.arc(p.x,p.y,5.5,0,7);ctx.strokeStyle='#FFF';ctx.lineWidth=2;ctx.stroke();
    ctx.beginPath();ctx.arc(p.x,p.y,2,0,7);ctx.fillStyle='#E4573D';ctx.fill();}}
let profGeom=null;
function drawProfile(){const c=cur,cv=$('#prof');const cssW=cv.clientWidth||900,W=cssW*1.6,H=cv.height=380;cv.width=W;
  const ctx=cv.getContext('2d');ctx.fillStyle='#0A100D';ctx.fillRect(0,0,W,H);
  const padL=46,padR=10,padT=26,padB=26,iw=W-padL-padR,ih=H-padT-padB,GAP=40;
  const total=c.profiles.reduce((s,p)=>s+p.len,0)+GAP*(c.profiles.length-1);
  const e0=Math.floor(c.emin/5)*5,e1=Math.ceil(c.emax/5)*5;
  const X=d=>padL+d/total*iw,Y=e=>padT+ih*(1-(e-e0)/(e1-e0));
  ctx.font='10px ui-monospace,Menlo,monospace';ctx.fillStyle='#8CA093';
  const step=(e1-e0)>60?20:(e1-e0)>25?10:5;
  for(let e=e0;e<=e1;e+=step){ctx.strokeStyle='rgba(34,53,43,0.7)';ctx.lineWidth=1;
    ctx.beginPath();ctx.moveTo(padL,Y(e));ctx.lineTo(W-padR,Y(e));ctx.stroke();ctx.fillText(e+' m',6,Y(e)+3);}
  [[c.min.v,'#7FB7DE','low'],[c.max.v,'#E4573D','high']].forEach(([v,col,lb])=>{ctx.strokeStyle=col;ctx.setLineDash([3,4]);ctx.lineWidth=1;
    ctx.beginPath();ctx.moveTo(padL,Y(v));ctx.lineTo(W-padR,Y(v));ctx.stroke();ctx.setLineDash([]);ctx.fillStyle=col;ctx.fillText(lb+' '+v.toFixed(0),W-padR-58,Y(v)-4);});
  let dcur=0;profGeom=[];
  c.profiles.forEach((p,i)=>{const x0=X(dcur),x1=X(dcur+p.len);
    if(i%2){ctx.fillStyle='rgba(230,224,204,0.035)';ctx.fillRect(x0,padT,x1-x0,ih);}
    ctx.fillStyle=HOLECOL[i%18];ctx.textAlign='center';ctx.font='bold 10px ui-monospace,Menlo,monospace';ctx.fillText(p.ref,(x0+x1)/2,padT-12);
    if(p.par){ctx.fillStyle='#8CA093';ctx.font='9px ui-monospace,Menlo,monospace';ctx.fillText('p'+p.par,(x0+x1)/2,padT-2);}ctx.textAlign='left';
    const n=p.e.length;ctx.strokeStyle=HOLECOL[i%18];ctx.lineWidth=1.7;ctx.beginPath();
    for(let k=0;k<n;k++){const d=dcur+p.len*k/(n-1),ev=c.emin+p.e[k]/10;k?ctx.lineTo(X(d),Y(ev)):ctx.moveTo(X(d),Y(ev));}ctx.stroke();
    ctx.lineTo(X(dcur+p.len),Y(e0));ctx.lineTo(X(dcur),Y(e0));ctx.closePath();ctx.fillStyle=HOLECOL[i%18]+'22';ctx.fill();
    profGeom.push({i,d0:dcur,d1:dcur+p.len,p});dcur+=p.len+GAP;});
  cv.__geom={X,Y,total,padL,padT,ih,iw,e0,e1};}
function profHover(ev){const cv=$('#prof'),r=cv.getBoundingClientRect(),mx=(ev.clientX-r.left)*(cv.width/r.width),g=cv.__geom;if(!g)return;
  const d=(mx-g.padL)/g.iw*g.total,seg=profGeom.find(s=>d>=s.d0&&d<=s.d1);
  if(!seg){hoverPt=null;$('#readout').innerHTML='&nbsp;';drawMap();return;}
  const t=d-seg.d0,p=seg.p,n=p.e.length,k=Math.max(0,Math.min(n-1,Math.round(t/p.len*(n-1)))),ev_m=cur.emin+p.e[k]/10;
  hoverPt=pointAlong(cur,seg.i,t);
  $('#readout').textContent=`hole ${p.ref}${p.par?' · par '+p.par:''}${p.name?' · '+p.name:''} — ${t.toFixed(0)} m in · ${ev_m.toFixed(1)} m elev`;drawMap();}
function fmt(n){return n.toLocaleString('en-US',{maximumFractionDigits:0});}
function select(k){cur=course(k);if(!cur.timg)cur.timg=renderTerrain(cur);
  document.querySelectorAll('.card').forEach(el=>el.classList.toggle('on',el.dataset.k===k));
  $('#cname').textContent=cur.label;$('#carch').textContent=cur.arch||'';
  const totalLen=cur.profiles.reduce((s,p)=>s+p.len,0),totalPar=cur.profiles.reduce((s,p)=>s+(p.par||0),0);
  let drops=cur.profiles.map(p=>({r:p.ref,d:(p.e[0]-p.e[p.e.length-1])/10}));drops.sort((a,b)=>Math.abs(b.d)-Math.abs(a.d));const bd=drops[0]||{d:0,r:'-'};
  $('#stats').innerHTML=`
    <div class="stat"><div class="k">High point</div><div class="v">${cur.max.v.toFixed(1)} <small>m</small></div></div>
    <div class="stat"><div class="k">Low point</div><div class="v">${cur.min.v.toFixed(1)} <small>m</small></div></div>
    <div class="stat"><div class="k">Total relief</div><div class="v">${(cur.emax-cur.emin).toFixed(1)} <small>m</small></div></div>
    <div class="stat"><div class="k">Routing length</div><div class="v">${fmt(totalLen)} <small>m</small></div></div>
    <div class="stat"><div class="k">Par</div><div class="v">${totalPar||'—'}</div></div>
    <div class="stat"><div class="k">Biggest hole Δ</div><div class="v">${bd.d>0?'−':'+'}${Math.abs(bd.d).toFixed(1)} <small>m · #${bd.r}</small></div></div>
    <div class="stat"><div class="k">Tree canopy</div><div class="v">${cur.treepct.toFixed(0)}<small>%</small></div></div>
    <div class="stat"><div class="k">Water</div><div class="v">${cur.waterpct.toFixed(1)}<small>%</small></div></div>`;
  $('#mapcap').textContent=`Survey extent ${fmt(cur.wm)} × ${fmt(cur.hm)} m · ${cur.holes.length} holes · tee ▪ green ⚑ · hillshade normalised to course relief`;
  hoverPt=null;drawMap();drawProfile();}
function boot(){const rail=$('#rail');let total=0;
  GROUPS.forEach(g=>{if(!g.keys.length)return;const h=document.createElement('div');h.className='ghdr';
    h.textContent=g.t+'  ('+g.keys.length+')';rail.appendChild(h);
    g.keys.forEach(k=>{const c=COURSES[k];total++;const el=document.createElement('div');el.className='card';el.dataset.k=k;
      el.innerHTML=`<div class="nm">${c.label}</div><div class="ar">${c.arch||''}</div>
        <div class="rl">relief ${(c.emax-c.emin).toFixed(0)} m · canopy ${c.treepct.toFixed(0)}% · ${c.holes.length} holes</div>`;
      el.onclick=()=>select(k);rail.appendChild(el);});});
  $('#subcount').textContent=total+' parkland courses · real elevation, canopy, water & line of play';
  const tg=(id,key)=>{$(id).onclick=()=>{LAYERS[key]=!LAYERS[key];$(id).classList.toggle('on',LAYERS[key]);drawMap();};};
  tg('#tgTerrain','terrain');tg('#tgTree','tree');tg('#tgWater','water');tg('#tgHoles','holes');
  $('#prof').addEventListener('mousemove',profHover);
  $('#prof').addEventListener('mouseleave',()=>{hoverPt=null;$('#readout').innerHTML='&nbsp;';drawMap();});
  $('#datestamp').textContent='retrieved '+new Date().toISOString().slice(0,10);
  window.addEventListener('resize',()=>{if(cur){drawMap();drawProfile();}});
  select(ORDER[0]);}
boot();
</script></body></html>
"""

GROUP_LABELS = [
    ("lowland", "LOWLAND — under 30 m of relief"),
    ("rolling", "ROLLING — the classic parkland band"),
    ("mountain", "MOUNTAIN & CANYON — over 80 m"),
]


def emit_html(courses, out_path):
    """courses: {key: record}. Writes a self-contained HTML viewer."""
    # strip fields the viewer doesn't use to keep the file lean
    slim = {}
    for k, c in courses.items():
        slim[k] = {f: c[f] for f in
                   ("label", "arch", "bbox", "b64", "tb64", "wb64", "treepct",
                    "waterpct", "wm", "hm", "emin", "emax", "min", "max",
                    "holes", "profiles") if f in c}
    from common import relief_group
    def gof(c):
        return c.get("group") or relief_group(c["emin"], c["emax"])
    groups = []
    for gkey, glabel in GROUP_LABELS:
        keys = [k for k, c in courses.items() if gof(c) == gkey]
        keys.sort(key=lambda k: courses[k]["emax"] - courses[k]["emin"])
        groups.append({"t": glabel, "keys": keys})

    html = (_HEAD + _BODY) \
        .replace("__COURSES__", json.dumps(slim, separators=(",", ":"))) \
        .replace("__GROUPS__", json.dumps(groups, separators=(",", ":")))
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(html)
    return out_path
