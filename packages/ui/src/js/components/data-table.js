import { define, emit, h } from '../util.js';

/**
 * <plbls-data-table caption="Utilisateurs"></plbls-data-table>
 *
 *   table.columns = [{ key: 'nom', label: 'Nom', sortable: true }, …];
 *   table.rows    = [{ nom: 'Ada', … }, …];
 *
 * Événement : `plbls-row-click`, detail = la ligne (clic, Entrée ou Espace).
 * Tri : numérique pour les nombres, alphabétique localisé (accents) sinon.
 */
export class PlblsDataTable extends HTMLElement {
  static observedAttributes = ['caption'];

  #columns = [];
  #rows = [];
  #sortKey = null;
  #asc = true;

  connectedCallback() {
    this.#render();
  }

  attributeChangedCallback() {
    if (this.isConnected) this.#render();
  }

  get columns() {
    return this.#columns;
  }
  set columns(v) {
    this.#columns = v ?? [];
    this.#render();
  }
  get rows() {
    return this.#rows;
  }
  set rows(v) {
    this.#rows = v ?? [];
    this.#render();
  }

  #compare(a, b) {
    if (a == null && b == null) return 0;
    if (a == null) return -1;
    if (b == null) return 1;
    if (typeof a === 'number' && typeof b === 'number') return a - b;
    return String(a).localeCompare(String(b), 'fr', { numeric: true, sensitivity: 'base' });
  }

  #sort(key) {
    if (this.#sortKey === key) this.#asc = !this.#asc;
    else {
      this.#sortKey = key;
      this.#asc = true;
    }
    this.#render();
  }

  #render() {
    const key = this.#sortKey;
    const rows = [...this.#rows];
    if (key) rows.sort((a, b) => (this.#asc ? 1 : -1) * this.#compare(a[key], b[key]));

    const head = this.#columns.map((col) => {
      const sorted = key === col.key ? (this.#asc ? 'ascending' : 'descending') : 'none';
      const th = h('th', { scope: 'col', 'aria-sort': col.sortable ? sorted : null });
      if (col.sortable) {
        const btn = h('button', { type: 'button', class: 'plbls-table__tri' }, col.label, key === col.key ? h('span', { class: 'plbls-table__fleche', 'aria-hidden': 'true' }, this.#asc ? '↑' : '↓') : null);
        btn.addEventListener('click', () => this.#sort(col.key));
        th.append(btn);
      } else th.append(col.label);
      return th;
    });

    const body = rows.map((row) => {
      const tr = h('tr', { tabindex: 0 }, this.#columns.map((col) => h('td', {}, row[col.key] == null ? '' : String(row[col.key]))));
      const fire = () => emit(this, 'plbls-row-click', row);
      tr.addEventListener('click', fire);
      tr.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          fire();
        }
      });
      return tr;
    });

    const caption = this.getAttribute('caption');
    this.replaceChildren(
      h(
        'table',
        { class: 'plbls-table plbls-table--cliquable' },
        caption ? h('caption', { class: 'plbls-table__legende' }, caption) : null,
        h('thead', {}, h('tr', {}, head)),
        h('tbody', {}, body),
      ),
    );
  }
}

define('plbls-data-table', PlblsDataTable);
