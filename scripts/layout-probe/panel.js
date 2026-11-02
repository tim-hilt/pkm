async page => {
  return JSON.stringify(await page.evaluate(() => {
    const r1 = n => Math.round(n*100)/100;
    const root = document.querySelector('.selected-shape-actions') ;
    if (!root) return null;
    const out = [];
    const ctl = el => {
      const r = el.getBoundingClientRect(); const cs = getComputedStyle(el);
      const inp = el.querySelector('input') || (el.tagName==='INPUT'?el:null);
      const svg = el.querySelector('svg');
      const sr = svg && svg.getBoundingClientRect();
      return {t:el.getAttribute('title')||el.getAttribute('aria-label')||inp?.getAttribute('title')||inp?.getAttribute('aria-label')||el.textContent.trim().slice(0,20)||'',
        rect:[r1(r.x),r1(r.y),r1(r.width),r1(r.height)], bg:cs.backgroundColor, br:cs.borderRadius, border:cs.border.replace('rgb(','rgb('), 
        checked: inp? (inp.checked??null) : null, cls:(el.className&&el.className.baseVal===undefined?el.className:'').toString().slice(0,60), svg: sr?[r1(sr.width),r1(sr.height)]:null, color: cs.color};
    };
    for (const fs of root.querySelectorAll('fieldset')) {
      const lg = fs.querySelector('legend'); const fr = fs.getBoundingClientRect();
      const items = [...fs.querySelectorAll('label.ToolIcon, .ToolIcon__icon, button, .color-picker__top-picks .color-picker__button, input[type=range], .color-picker-trigger, .color-picker__button, select, .picker-trigger, .picker, [role=radio], .buttonList > *')].filter((e,i,a)=>true);
      out.push({legend: lg?.textContent ?? null, rect:[r1(fr.x),r1(fr.y),r1(fr.width),r1(fr.height)], items: items.map(ctl)});
    }
    return out;
  }));
}
