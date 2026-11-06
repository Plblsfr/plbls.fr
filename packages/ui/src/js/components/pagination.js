import { define, emit, h } from '../util.js';

/**
 * <plbls-pagination page="2" total="9"></plbls-pagination>
 * Événement : `plbls-change`, detail = nouvelle page. La page courante est
 * mise à jour par le composant (retirer l'attribut `controlled` pour la piloter
 * soi-même : il faut alors régler `page` en réponse à l'événement).
 */
export class PlblsPagination extends HTMLElement {
  static observedAttributes = ['page', 'total'];

  connectedCallback() {
    this.#render();
  }

  attributeChangedCallback() {
    if (this.isConnected) this.#render();
  }

  get page() {
    return Number(this.getAttribute('page')) || 1;
  }
  set page(v) {
    this.setAttribute('page', String(v));
  }
  get total() {
    return Number(this.getAttribute('total')) || 1;
  }
  set total(v) {
    this.setAttribute('total', String(v));
  }

  #go(p) {
    if (p < 1 || p > this.total) return;
    if (emit(this, 'plbls-change', p) && !this.hasAttribute('controlled')) this.page = p;
  }

  #render() {
    const arrow = (label, text, target, disabled) => {
      const b = h('button', { type: 'button', class: 'plbls-pagination__fleche', 'aria-label': label, disabled }, text);
      b.addEventListener('click', () => this.#go(target));
      return b;
    };
    this.replaceChildren(
      h(
        'nav',
        { class: 'plbls-pagination', 'aria-label': 'Pagination' },
        arrow('Page précédente', '←', this.page - 1, this.page <= 1),
        h('span', { class: 'plbls-pagination__etat' }, `${this.page} / ${this.total}`),
        arrow('Page suivante', '→', this.page + 1, this.page >= this.total),
      ),
    );
  }
}

define('plbls-pagination', PlblsPagination);
