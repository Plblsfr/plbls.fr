import { define, h } from '../util.js';

/**
 * <plbls-toast-container></plbls-toast-container>   (à poser une fois ; créé tout seul sinon)
 *
 * import { toast } from '@plbls/ui'   |   window.plbls.toast.success('Enregistré')
 */
export class PlblsToastContainer extends HTMLElement {
  #timers = new Map();
  #seq = 0;

  connectedCallback() {
    this.classList.add('plbls-toast-pile');
  }

  disconnectedCallback() {
    this.clear();
  }

  show(text, type = 'info', durationMs = 4000) {
    const id = ++this.#seq;
    const error = type === 'erreur';
    const close = h('button', { type: 'button', class: 'plbls-toast__fermer', 'aria-label': 'Fermer' }, h('span', { 'aria-hidden': 'true' }, '✕'));
    const el = h(
      'div',
      { class: `plbls-toast${type === 'info' ? '' : ` plbls-toast--${type}`}`, role: error ? 'alert' : 'status', 'aria-live': error ? 'assertive' : 'polite', 'data-id': id },
      h('span', {}, text),
      close,
    );
    close.addEventListener('click', () => this.dismiss(id));
    this.append(el);
    if (durationMs > 0) this.#timers.set(id, setTimeout(() => this.dismiss(id), durationMs));
    return id;
  }

  dismiss(id) {
    clearTimeout(this.#timers.get(id));
    this.#timers.delete(id);
    this.querySelector(`[data-id="${id}"]`)?.remove();
  }

  clear() {
    this.#timers.forEach(clearTimeout);
    this.#timers.clear();
    this.replaceChildren();
  }
}

define('plbls-toast-container', PlblsToastContainer);

function container() {
  let el = document.querySelector('plbls-toast-container');
  if (!el) {
    el = document.createElement('plbls-toast-container');
    document.body.append(el);
  }
  return el;
}

export const toast = {
  /** @param {'info'|'succes'|'erreur'} [type] */
  show: (text, type = 'info', durationMs = 4000) => container().show(text, type, durationMs),
  success: (text) => toast.show(text, 'succes'),
  error: (text) => toast.show(text, 'erreur'),
  info: (text) => toast.show(text, 'info'),
  remove: (id) => container().dismiss(id),
  clear: () => container().clear(),
};
