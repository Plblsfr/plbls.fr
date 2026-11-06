// Comportements globaux, sans balise dédiée : infobulles et barre latérale.
// Délégation sur `document` — le balisage ajouté après coup fonctionne aussi.

let tip = null;
let tipId = 0;

function showTip(host) {
  const text = host.getAttribute('data-plbls-tooltip');
  if (!text || tip) return;
  const position = host.getAttribute('data-plbls-tooltip-position') ?? 'top';

  tip = document.createElement('span');
  tip.id = `plbls-tooltip-${++tipId}`;
  tip.setAttribute('role', 'tooltip');
  tip.className = `plbls-tooltip plbls-tooltip--${position}`;
  tip.textContent = text;
  document.body.append(tip);

  const h = host.getBoundingClientRect();
  const t = tip.getBoundingClientRect();
  const gap = 8;
  const midX = h.left + scrollX + h.width / 2 - t.width / 2;
  const midY = h.top + scrollY + h.height / 2 - t.height / 2;
  const [top, left] = {
    bottom: [h.bottom + scrollY + gap, midX],
    left: [midY, h.left + scrollX - t.width - gap],
    right: [midY, h.right + scrollX + gap],
    top: [h.top + scrollY - t.height - gap, midX],
  }[position] ?? [h.top + scrollY - t.height - gap, midX];
  tip.style.top = `${top}px`;
  tip.style.left = `${left}px`;
  host.setAttribute('aria-describedby', tip.id);
  tip.host = host;
}

function hideTip() {
  if (!tip) return;
  tip.host?.removeAttribute('aria-describedby');
  tip.remove();
  tip = null;
}

const hostOf = (e) => (e.target instanceof Element ? e.target.closest('[data-plbls-tooltip]') : null);

let installed = false;

export function installBehaviors() {
  if (installed || typeof document === 'undefined') return;
  installed = true;

  for (const type of ['mouseover', 'focusin']) {
    document.addEventListener(type, (e) => {
      const host = hostOf(e);
      if (host) showTip(host);
    });
  }
  for (const type of ['mouseout', 'focusout']) {
    document.addEventListener(type, (e) => {
      if (hostOf(e)) hideTip();
    });
  }
  document.addEventListener('keydown', (e) => e.key === 'Escape' && hideTip());

  // Barre latérale repliable (mobile)
  document.addEventListener('click', (e) => {
    const button = e.target instanceof Element ? e.target.closest('.plbls-sidebar__bascule') : null;
    if (!button) return;
    const sidebar = button.closest('.plbls-sidebar');
    const open = sidebar.classList.toggle('is-open');
    button.setAttribute('aria-expanded', String(open));
    button.textContent = open ? 'Fermer' : 'Menu';
  });
}
