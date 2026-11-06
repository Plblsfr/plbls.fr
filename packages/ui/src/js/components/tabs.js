import { define, emit } from '../util.js';

/**
 * <plbls-tabs>
 *   <div class="plbls-tabs" role="tablist">
 *     <button class="plbls-tabs__onglet is-active" role="tab" id="t-a" aria-controls="p-a" aria-selected="true">Profil</button>
 *     <button class="plbls-tabs__onglet" role="tab" id="t-b" aria-controls="p-b" aria-selected="false">Sécurité</button>
 *   </div>
 * </plbls-tabs>
 * <div id="p-a" role="tabpanel">…</div>
 * <div id="p-b" role="tabpanel" hidden>…</div>
 *
 * Les panneaux (optionnels) sont reliés par `aria-controls` et affichés /
 * masqués. Événement : `plbls-change`, detail = id de l'onglet activé.
 * Clavier : ← → Home End, avec focus itinérant (tabindex).
 */
export class PlblsTabs extends HTMLElement {
  connectedCallback() {
    this.style.display = 'contents';
    this.addEventListener('click', this.#onClick);
    this.addEventListener('keydown', this.#onKey);
    const current = this.#tabs().find((t) => t.getAttribute('aria-selected') === 'true' || t.classList.contains('is-active'));
    this.#select(current ?? this.#tabs()[0], false);
  }

  disconnectedCallback() {
    this.removeEventListener('click', this.#onClick);
    this.removeEventListener('keydown', this.#onKey);
  }

  #tabs() {
    return Array.from(this.querySelectorAll('[role="tab"]'));
  }

  #onClick = (e) => {
    const tab = e.target.closest('[role="tab"]');
    if (tab && this.contains(tab)) this.#select(tab, true);
  };

  #onKey = (e) => {
    const tabs = this.#tabs();
    const i = tabs.indexOf(document.activeElement);
    if (i === -1) return;
    const next = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1 }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    const target = tabs[(next + tabs.length) % tabs.length];
    target.focus();
    this.#select(target, true);
  };

  #select(selected, notify) {
    if (!selected) return;
    for (const tab of this.#tabs()) {
      const on = tab === selected;
      tab.classList.toggle('is-active', on);
      tab.setAttribute('aria-selected', String(on));
      tab.tabIndex = on ? 0 : -1;
      const panel = tab.getAttribute('aria-controls') && document.getElementById(tab.getAttribute('aria-controls'));
      if (panel) panel.hidden = !on;
    }
    if (notify) emit(this, 'plbls-change', selected.id || selected.dataset.tab || selected.textContent.trim());
  }
}

define('plbls-tabs', PlblsTabs);
