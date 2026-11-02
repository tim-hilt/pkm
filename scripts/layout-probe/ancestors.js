async page => JSON.stringify(await page.evaluate(() => {
  const el = document.querySelector('.properties-content, .color-picker-content, .color-picker__heading');
  const out=[]; let e=el;
  while(e && e!==document.body){ const r=e.getBoundingClientRect(), cs=getComputedStyle(e); out.push([e.tagName,String(e.className).slice(0,60),[r.x,r.y,r.width,r.height].map(n=>Math.round(n*10)/10),cs.backgroundColor,cs.borderRadius,cs.boxShadow.slice(0,90),cs.padding]); e=e.parentElement;}
  return out;
}))
