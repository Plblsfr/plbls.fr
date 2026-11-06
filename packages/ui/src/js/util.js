let seq = 0;

/** Identifiant unique pour relier labels et champs (aria-*). */
export function uid(prefix = 'plbls') {
  return `${prefix}-${++seq}`;
}

/** Crée un élément : h('button', { class: 'x', type: 'button' }, 'texte'). */
export function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === false || value == null) continue;
    el.setAttribute(name, value === true ? '' : String(value));
  }
  for (const child of children.flat()) {
    if (child == null || child === false) continue;
    el.append(child);
  }
  return el;
}

export const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function focusables(root) {
  return Array.from(root.querySelectorAll(FOCUSABLE)).filter((el) => el.offsetParent !== null);
}

/** Attribut booléen HTML : présent = vrai, sauf "false". */
export function boolAttr(el, name) {
  return el.hasAttribute(name) && el.getAttribute(name) !== 'false';
}

export function emit(el, name, detail) {
  return el.dispatchEvent(new CustomEvent(name, { detail, bubbles: true, composed: true, cancelable: true }));
}

/** Enregistre un custom element une seule fois (import multiple, HMR…). */
export function define(name, ctor) {
  if (!customElements.get(name)) customElements.define(name, ctor);
}
