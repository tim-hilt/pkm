async page => {
  const SELS = __SELS__;
  return JSON.stringify(await page.evaluate((SELS) => {
    const props = ['backgroundColor','color','borderRadius','border','boxShadow','padding','margin','gap','fontSize','fontFamily','fontWeight','opacity','backdropFilter','position'];
    const skip = new Set(['none','normal','0px','rgba(0, 0, 0, 0)','0px 0px 0px 0px','static','0px 0px 0px 0px / 0px 0px 0px 0px','1','0px none rgb(0, 0, 0)']);
    const r1 = n => Math.round(n*100)/100;
    const info = (el) => {
      const r = el.getBoundingClientRect(); const cs = getComputedStyle(el);
      const o = {rect:[r1(r.x),r1(r.y),r1(r.width),r1(r.height)]};
      for (const p of props) { const v = cs[p]; if (v && !skip.has(v) && !/^0px none/.test(v)) o[p]=v; }
      const svg = el.querySelector('svg'); if (svg) { const s = svg.getBoundingClientRect(); o.svg=[r1(s.width),r1(s.height)]; }
      const t = el.getAttribute('title')||el.getAttribute('aria-label'); if (t) o.label=t;
      const tx = (el.children.length===0 && el.textContent||'').trim(); if (tx) o.text=tx.slice(0,40);
      return o;
    };
    const res = {};
    for (const [name, sel, all] of SELS) {
      const els = [...document.querySelectorAll(sel)];
      if (!els.length) { res[name] = null; continue; }
      res[name] = all ? els.map(info) : info(els[0]);
    }
    res.__vars = {};
    const cs = getComputedStyle(document.querySelector('.excalidraw'));
    for (const v of ['--space-factor','--default-border-radius','--border-radius-md','--border-radius-lg','--default-button-size','--default-icon-size','--lg-button-size','--lg-icon-size','--island-bg-color','--icon-fill-color','--button-hover-bg','--button-active-bg','--color-primary','--color-surface-lowest','--color-surface-low','--color-surface-mid','--color-surface-high','--color-surface-primary-container','--shadow-island','--sidebar-shadow','--popup-bg-color','--text-primary-color','--color-on-primary-container','--input-bg-color','--input-border-color','--input-hover-bg-color','--button-gray-1','--button-gray-2','--button-gray-3','--color-border-outline','--color-gray-10','--color-gray-20','--color-gray-30','--color-gray-40','--color-gray-50','--color-gray-60','--color-gray-70','--color-gray-80','--color-gray-90','--color-gray-100','--color-primary-darker','--color-primary-darkest','--color-primary-light','--color-primary-light-darker','--color-selection','--color-danger','--color-warning']) res.__vars[v] = cs.getPropertyValue(v).trim();
    res.__viewport = [innerWidth, innerHeight, devicePixelRatio];
    return res;
  }, SELS));
}
