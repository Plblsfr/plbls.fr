import { define, emit, h } from '../util.js';

/**
 * <plbls-dropdown-menu>
 *   <button slot="trigger">⋯</button>
 *   <button data-action="rename">Renommer</button>
 *   <button data-action="delete" data-danger>Supprimer</button>
 * </plbls-dropdown-menu>
 *
 * Les enfants sans `slot="trigger"` deviennent les items du menu.
 * Événement : `plbls-action`, detail = valeur de `data-action`.
 * Clavier   : ↑ ↓ Home End naviguent, Échap ferme et rend le focus au déclencheur.
 */
export class PlblsDropdownMenu extends HTMLElement {
  #trigger = null;
  #menu = null;
  #items = [];
  #onDocClick = (e) => {
    if (!this.contains(e.target)) this.#toggle(false);
  };

  connectedCallback() {
    if (this.#trigger) return;
    const children = Array.from(this.children);
    const slotted = children.find((c) => c.getAttribute('slot') === 'trigger');
    this.#items = children.filter((c) => c !== slotted);

    this.#trigger = h('button', { type: 'button', class: 'plbls-dropdown__declencheur', 'aria-haspopup': 'menu', 'aria-expanded': 'false' });
    if (slotted) {
      slotted.removeAttribute('slot');
      this.#trigger.append(...slotted.childNodes);
    }
    this.#trigger.addEventListener('click', () => this.#toggle());

    this.#menu = h('div', { class: 'plbls-dropdown__menu', role: 'menu' });
    this.#menu.hidden = true;
    for (const src of this.#items) {
      const item = h(
        'button',
        { type: 'button', role: 'menuitem', class: 'plbls-dropdown__item' + (src.hasAttribute('data-danger') ? ' plbls-dropdown__item--danger' : ''), 'data-action': src.dataset.action },
        src.textContent.trim(),
      );
      item.addEventListener('click', () => {
        this.#toggle(false);
        emit(this, 'plbls-action', src.dataset.action);
      });
      this.#menu.append(item);
    }

    this.classList.add('plbls-dropdown');
    this.replaceChildren(this.#trigger, this.#menu);
    this.addEventListener('keydown', (e) => this.#onKey(e));
  }

  disconnectedCallback() {
    document.removeEventListener('click', this.#onDocClick, true);
  }

  #options() {
    return Array.from(this.#menu.querySelectorAll('[role="menuitem"]'));
  }

  #toggle(force) {
    const open = force ?? this.#menu.hidden;
    this.#menu.hidden = !open;
    this.#trigger.setAttribute('aria-expanded', String(open));
    document.removeEventListener('click', this.#onDocClick, true);
    if (open) {
      document.addEventListener('click', this.#onDocClick, true);
      queueMicrotask(() => this.#options()[0]?.focus());
    }
  }

  #onKey(event) {
    if (this.#menu.hidden) return;
    const options = this.#options();
    if (!options.length) return;
    const index = options.indexOf(document.activeElement);
    const go = (i) => {
      event.preventDefault();
      options[(i + options.length) % options.length].focus();
    };
    switch (event.key) {
      case 'Escape':
        event.preventDefault();
        this.#toggle(false);
        this.#trigger.focus();
        break;
      case 'ArrowDown':
        go(index + 1);
        break;
      case 'ArrowUp':
        go(index - 1);
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(options.length - 1);
        break;
    }
  }
}

define('plbls-dropdown-menu', PlblsDropdownMenu);
