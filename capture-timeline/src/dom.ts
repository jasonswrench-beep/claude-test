// SPDX-License-Identifier: Apache-2.0
// A small helper for making elements, written the way OpenStage's own panels do.

export type Attrs = Record<string, string | number | boolean | undefined>;

export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: (Node | string)[]): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === undefined || value === false) continue;
    element.setAttribute(name, value === true ? "" : String(value));
  }
  element.append(...children);
  return element;
}

let counter = 0;

/** A new number for ids, so two panels on one page never share an id. */
export function nextId(prefix: string): string {
  counter += 1;
  return `${prefix}-${counter}`;
}
