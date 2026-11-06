import { define, h, uid } from '../util.js';

/**
 * <plbls-copy-field label="Lien de partage" value="https://…"></plbls-copy-field>
 */
export class PlblsCopyField extends HTMLElement {
  static observedAttributes = ['label', 'value'];

  #state = 'pret';
  #timer = 0;
  #id = uid('plbls-copy');

  connectedCallback() {
    this.#render();
  }

  disconnectedCallback() {
    clearTimeout(this.#timer);
  }

  attributeChangedCallback() {
    if (this.isConnected) this.#render();
  }

  async copy() {
    try {
      await navigator.clipboard.writeText(this.getAttribute('value') ?? '');
      this.#state = 'copie';
    } catch {
      this.#state = 'echec';
    }
    this.#render();
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      this.#state = 'pret';
      this.#render();
    }, 1800);
  }

  #render() {
    this.classList.add('plbls-copy-field');
    const label = this.getAttribute('label');
    const input = h('input', { class: 'plbls-copy-field__valeur', type: 'text', readonly: true, id: this.#id, 'aria-label': label ? null : 'Valeur à copier' });
    input.value = this.getAttribute('value') ?? '';
    input.addEventListener('click', () => input.select());
    const button = h('button', { class: 'plbls-copy-field__copier', type: 'button' }, { pret: 'Copier', copie: 'Copié', echec: 'Échec' }[this.#state]);
    button.addEventListener('click', () => this.copy());
    this.replaceChildren(
      label ? h('label', { class: 'plbls-copy-field__label', for: this.#id }, label) : null,
      h('div', { class: 'plbls-copy-field__ligne' }, input, button),
      h('span', { class: 'plbls-sr-only', 'aria-live': 'polite' }, this.#state === 'copie' ? 'Copié dans le presse-papier' : ''),
    );
  }
}

define('plbls-copy-field', PlblsCopyField);
