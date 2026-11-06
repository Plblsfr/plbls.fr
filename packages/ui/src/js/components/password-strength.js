import { define, h } from '../util.js';

const LABELS = ['Très faible', 'Faible', 'Moyen', 'Bon', 'Excellent'];

/** Score 0–4 : longueur, casse mixte, chiffre, caractère spécial. */
export function passwordScore(value) {
  let s = 0;
  if (value.length >= 8) s++;
  if (value.length >= 12) s++;
  if (/[A-Z]/.test(value) && /[a-z]/.test(value)) s++;
  if (/\d/.test(value)) s++;
  if (/[^A-Za-z0-9]/.test(value)) s++;
  return Math.min(s, 4);
}

/**
 * <plbls-password-strength value="…"></plbls-password-strength>
 * Pour suivre un champ : `strength.value = input.value` à chaque saisie, ou
 * `<plbls-password-strength for="mon-input">` qui écoute le champ lui-même.
 */
export class PlblsPasswordStrength extends HTMLElement {
  static observedAttributes = ['value', 'for'];

  #source = null;
  #onInput = () => this.setAttribute('value', this.#source.value);

  connectedCallback() {
    this.#bind();
    this.#render();
  }

  disconnectedCallback() {
    this.#source?.removeEventListener('input', this.#onInput);
  }

  attributeChangedCallback(name) {
    if (!this.isConnected) return;
    if (name === 'for') this.#bind();
    this.#render();
  }

  get value() {
    return this.getAttribute('value') ?? '';
  }
  set value(v) {
    this.setAttribute('value', v ?? '');
  }

  #bind() {
    this.#source?.removeEventListener('input', this.#onInput);
    const id = this.getAttribute('for');
    this.#source = id ? document.getElementById(id) : null;
    this.#source?.addEventListener('input', this.#onInput);
  }

  #render() {
    const value = this.value;
    if (!value) return this.replaceChildren();
    const score = passwordScore(value);
    const label = LABELS[score];
    this.replaceChildren(
      h(
        'div',
        { class: 'plbls-strength' },
        h(
          'div',
          { class: 'plbls-strength__barres', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': 4, 'aria-valuenow': score, 'aria-valuetext': label, 'aria-label': 'Force du mot de passe' },
          [0, 1, 2, 3].map((i) => h('span', { class: 'plbls-strength__barre' + (i < score ? ' is-filled' : '') })),
        ),
        h('span', { class: 'plbls-strength__libelle', 'aria-live': 'polite' }, label),
      ),
    );
  }
}

define('plbls-password-strength', PlblsPasswordStrength);
