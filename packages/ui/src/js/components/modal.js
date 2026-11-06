import { define, emit, focusables, h, uid } from '../util.js';

/**
 * <plbls-modal title="Titre" open>…contenu…</plbls-modal>
 *
 * Attributs : `open` (affiche), `title`.
 * Événement  : `plbls-close` (Échap, fond, bouton ✕). Si personne n'appelle
 *              preventDefault(), la modale retire elle-même `open`.
 * Clavier    : Échap ferme, Tab reste piégé, le focus revient à la fermeture.
 */
export class PlblsModal extends HTMLElement {
  static observedAttributes = ['open', 'title'];

  #body = null;
  #fond = null;
  #previous = null;
  #overflow = '';
  #titleId = uid('plbls-modal-titre');

  connectedCallback() {
    if (!this.#body) {
      this.#body = h('div', { class: 'plbls-modal__corps' });
      this.#body.append(...this.childNodes);
    }
    this.#sync();
  }

  disconnectedCallback() {
    this.#release();
  }

  attributeChangedCallback() {
    if (this.isConnected && this.#body) this.#sync();
  }

  get open() {
    return this.hasAttribute('open');
  }
  set open(value) {
    this.toggleAttribute('open', Boolean(value));
  }

  #sync() {
    if (!this.#fond) this.#build();
    this.#fond.querySelector('h3')?.replaceChildren(this.getAttribute('title') ?? '');
    const open = this.open;
    if (open === !this.#fond.hidden) return;
    this.#fond.hidden = !open;
    if (!open) return this.#release();

    this.#previous = document.activeElement;
    this.#overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    queueMicrotask(() => focusables(this.#fond)[0]?.focus());
  }

  // Le contenu reste dans le DOM (masqué) quand la modale est fermée : les
  // champs et boutons qu'elle contient restent atteignables par le script.
  #build() {
    const title = this.getAttribute('title');
    const close = h('button', { type: 'button', class: 'plbls-modal__fermer', 'aria-label': 'Fermer' }, h('span', { 'aria-hidden': 'true' }, '✕'));
    close.addEventListener('click', () => this.#requestClose());
    const panel = h(
      'div',
      { class: 'plbls-modal', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': this.#titleId, 'aria-label': title ? null : 'Boîte de dialogue' },
      h('div', { class: 'plbls-modal__entete' }, h('h3', { id: this.#titleId }), close),
      this.#body,
    );
    this.#fond = h('div', { class: 'plbls-modal__fond', hidden: true }, panel);
    this.#fond.addEventListener('click', (e) => {
      if (e.target === this.#fond) this.#requestClose();
    });
    this.#fond.addEventListener('keydown', (e) => this.#onKey(e));
    this.replaceChildren(this.#fond);
  }

  #release() {
    if (this.#previous === null) return;
    document.body.style.overflow = this.#overflow;
    this.#previous?.focus?.();
    this.#previous = null;
  }

  #requestClose() {
    if (emit(this, 'plbls-close')) this.open = false;
  }

  #onKey(event) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      this.#requestClose();
      return;
    }
    if (event.key !== 'Tab') return;
    const targets = focusables(this.#fond);
    if (!targets.length) return;
    const first = targets[0];
    const last = targets[targets.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
}

define('plbls-modal', PlblsModal);
