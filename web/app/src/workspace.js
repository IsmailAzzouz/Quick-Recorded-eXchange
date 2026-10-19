// Presentation state only: modes keep their DOM and in-tab work intact.
export function setupWorkspace(page, canSwitch = () => true) {
  const tabs = [...page.querySelectorAll('[role="tab"][data-mode]')];
  function select(mode, focus = false) {
    if (!canSwitch(mode)) return;
    for (const tab of tabs) {
      const selected = tab.dataset.mode === mode;
      tab.setAttribute('aria-selected', String(selected));
      tab.tabIndex = selected ? 0 : -1;
      page.getElementById(tab.getAttribute('aria-controls')).hidden = !selected;
      if (selected && focus) tab.focus();
    }
  }
  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => select(tab.dataset.mode));
    tab.addEventListener('keydown', event => {
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1
        : ['ArrowRight', 'ArrowDown'].includes(event.key) ? (index + 1) % tabs.length
        : ['ArrowLeft', 'ArrowUp'].includes(event.key) ? (index + tabs.length - 1) % tabs.length : -1;
      if (next < 0 || tabs[next].disabled) return;
      event.preventDefault();
      select(tabs[next].dataset.mode, true);
    });
  });

  const root = page.documentElement;
  const theme = page.getElementById('dark-mode');
  const preference = page.defaultView.matchMedia('(prefers-color-scheme: dark)');
  let explicit = null;
  try { explicit = localStorage.getItem('qrx.theme'); } catch { /* Optional preference storage. */ }
  function apply(dark) {
    root.dataset.theme = dark ? 'dark' : 'light';
    theme.checked = dark;
    page.querySelector('meta[name="theme-color"]').content = dark ? '#111111' : '#ffffff';
  }
  apply(explicit ? explicit === 'dark' : preference.matches);
  theme.addEventListener('change', () => {
    apply(theme.checked);
    explicit = theme.checked ? 'dark' : 'light';
    try { localStorage.setItem('qrx.theme', explicit); } catch { /* Optional preference storage. */ }
  });
  preference.addEventListener('change', () => { if (!explicit) apply(preference.matches); });
  return { select };
}
