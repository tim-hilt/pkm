async page => {
  return await page.evaluate(() => {
    const out = [];
    const root = document.querySelector('.layer-ui__wrapper') || document.querySelector('.excalidraw');
    const walk = (el, d) => {
      if (d > 9) return;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) return;
      const cls = (el.getAttribute('class')||'').split(/\s+/).slice(0,4).join('.');
      out.push(`${'  '.repeat(d)}${el.tagName.toLowerCase()}.${cls} [${Math.round(r.x*10)/10},${Math.round(r.y*10)/10} ${Math.round(r.width*10)/10}x${Math.round(r.height*10)/10}]${el.getAttribute('title')?' title='+el.getAttribute('title'):''}${el.getAttribute('aria-label')?' aria='+el.getAttribute('aria-label'):''}`);
      for (const c of el.children) walk(c, d+1);
    };
    walk(document.querySelector('.excalidraw'), 0);
    return out.join('\n');
  });
}
