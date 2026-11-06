import { boolAttr, define, emit, h } from '../util.js';

/**
 * <plbls-dropzone accept="image/*" multiple text="Glisse tes fichiers ici…"></plbls-dropzone>
 * Événement : `plbls-files`, detail = File[] (dépôt ou sélection).
 */
export class PlblsDropzone extends HTMLElement {
  static observedAttributes = ['accept', 'multiple', 'text'];

  #depth = 0;

  connectedCallback() {
    this.#render();
  }

  attributeChangedCallback() {
    if (this.isConnected) this.#render();
  }

  #render() {
    const input = h('input', { type: 'file', class: 'plbls-dropzone__entree', accept: this.getAttribute('accept') ?? '*', multiple: boolAttr(this, 'multiple') });
    input.addEventListener('change', () => {
      if (input.files?.length) emit(this, 'plbls-files', Array.from(input.files));
      input.value = '';
    });
    // un compteur évite le clignotement du surlignage entre éléments enfants
    const zone = h(
      'label',
      { class: 'plbls-dropzone' },
      input,
      h('span', { class: 'plbls-dropzone__icone', 'aria-hidden': 'true' }, '↑'),
      h('span', { class: 'plbls-dropzone__texte' }, this.getAttribute('text') ?? 'Glisse tes fichiers ici, ou clique pour choisir'),
    );
    zone.addEventListener('dragenter', (e) => {
      e.preventDefault();
      this.#depth++;
      zone.classList.add('is-over');
    });
    zone.addEventListener('dragover', (e) => e.preventDefault());
    zone.addEventListener('dragleave', () => {
      this.#depth = Math.max(0, this.#depth - 1);
      if (!this.#depth) zone.classList.remove('is-over');
    });
    zone.addEventListener('drop', (e) => {
      e.preventDefault();
      this.#depth = 0;
      zone.classList.remove('is-over');
      const files = e.dataTransfer?.files;
      if (files?.length) emit(this, 'plbls-files', Array.from(files));
    });
    this.replaceChildren(zone);
  }
}

define('plbls-dropzone', PlblsDropzone);
