async page => JSON.stringify(await page.evaluate(() => {
  const out = [];
  const sections = {
    toolbar: '.App-toolbar, .mobile-toolbar', footer: '.layer-ui__wrapper__footer, .mobile-shape-actions .undo-button, .compact-action-item', topleft:'.excalidraw-ui-top-left, .excalidraw-ui-top-right, .App-top-bar',
    panel: '.selected-shape-actions, .compact-shape-actions, .properties-content', menu: '.dropdown-menu-container', tool: '.tool-popover-content', picker:'.color-picker, .FontPicker__container', iconpicker:'.picker'
  };
  for (const [sec, sel] of Object.entries(sections)) {
    for (const root of document.querySelectorAll(sel)) {
      for (const el of root.querySelectorAll('label, button, .dropdown-menu-item, a, [role=radio]')) {
        const inp = el.querySelector('input');
        const name = (el.getAttribute('title') || el.getAttribute('aria-label') || inp?.getAttribute('title') || inp?.getAttribute('aria-label') || el.textContent.trim()).split(' — ')[0].split(' - ')[0].slice(0, 40);
        const svgs = [...el.querySelectorAll('svg')];
        const svg = svgs[0];
        if (!svg) continue;
        const group = el.closest('fieldset')?.querySelector('legend')?.textContent || '';
        out.push({ sec, group, name, w: svg.getBoundingClientRect().width, vb: svg.getAttribute('viewBox'), svg: svg.outerHTML });
      }
    }
  }
  return out;
}))
