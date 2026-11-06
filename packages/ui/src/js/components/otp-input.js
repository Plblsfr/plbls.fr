import { boolAttr, define, emit, h } from '../util.js';

/**
 * <plbls-otp-input length="6" error></plbls-otp-input>
 * Événement : `plbls-complete`, detail = code saisi. Propriété `value` en lecture.
 */
export class PlblsOtpInput extends HTMLElement {
  static observedAttributes = ['length', 'error'];

  #cells = [];

  connectedCallback() {
    this.#build();
  }

  attributeChangedCallback(name) {
    if (!this.isConnected) return;
    if (name === 'length') this.#build();
    else this.classList.toggle('plbls-otp--error', boolAttr(this, 'error'));
  }

  get length() {
    return Math.max(1, Number(this.getAttribute('length')) || 6);
  }

  get value() {
    return this.#cells.map((c) => c.value).join('');
  }

  #build() {
    const previous = this.#cells.map((c) => c.value);
    this.classList.add('plbls-otp');
    this.classList.toggle('plbls-otp--error', boolAttr(this, 'error'));
    this.#cells = Array.from({ length: this.length }, (_, i) => {
      const cell = h('input', {
        class: 'plbls-otp__case',
        type: 'text',
        inputmode: 'numeric',
        autocomplete: i === 0 ? 'one-time-code' : 'off',
        maxlength: 1,
        'aria-label': `Chiffre ${i + 1}`,
      });
      cell.value = previous[i] ?? '';
      cell.addEventListener('input', () => this.#onInput(i));
      cell.addEventListener('keydown', (e) => this.#onKey(i, e));
      return cell;
    });
    this.replaceChildren(...this.#cells);
    this.addEventListener('paste', this.#onPaste);
  }

  #onInput(i) {
    const cell = this.#cells[i];
    cell.value = cell.value.slice(-1).replace(/\D/g, '');
    if (cell.value && i < this.length - 1) this.#cells[i + 1].focus();
    this.#maybeComplete();
  }

  #onKey(i, e) {
    const target = {
      Backspace: !this.#cells[i].value && i > 0 ? i - 1 : null,
      ArrowLeft: i > 0 ? i - 1 : null,
      ArrowRight: i < this.length - 1 ? i + 1 : null,
    }[e.key];
    if (target != null) {
      e.preventDefault();
      this.#cells[target].focus();
    }
  }

  #onPaste = (e) => {
    e.preventDefault();
    const text = (e.clipboardData?.getData('text') ?? '').replace(/\D/g, '');
    this.#cells.forEach((c, i) => (c.value = text[i] ?? ''));
    const next = this.#cells.findIndex((c) => !c.value);
    this.#cells[next === -1 ? this.length - 1 : next].focus();
    this.#maybeComplete();
  };

  #maybeComplete() {
    if (this.#cells.every((c) => c.value)) emit(this, 'plbls-complete', this.value);
  }
}

define('plbls-otp-input', PlblsOtpInput);
