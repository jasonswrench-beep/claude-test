// SPDX-License-Identifier: Apache-2.0
/**
 * The one place the editing screens say things: a visible line of text that is also a polite live region, so a
 * screen reader reads each sentence once. The clip list and the timeline share one, so what an author hears does not
 * depend on which of the two they are in.
 */

export class Announcer {
  readonly element: HTMLElement;
  #timer: number | undefined;
  readonly #also: ((message: string) => void) | undefined;

  /** `also` is told every message too (for example OpenStage's own announcer). */
  constructor(also?: (message: string) => void) {
    this.#also = also;
    const status = document.createElement("p");
    status.className = "os-cl-status";
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    status.setAttribute("aria-atomic", "true");
    status.textContent = "Ready. Choose a row or a clip with the arrow keys.";
    this.element = status;
  }

  /**
   * Say something. The line is cleared first and filled a moment later, so the same sentence said twice is still read, and
   * so it comes after whatever a screen reader says for a change of focus rather than being talked over by it.
   */
  say(message: string): void {
    if (this.#timer !== undefined) window.clearTimeout(this.#timer);
    this.element.textContent = "";
    this.#timer = window.setTimeout(() => {
      this.element.textContent = message;
      this.#timer = undefined;
    }, 60);
    this.#also?.(message);
  }

  destroy(): void {
    if (this.#timer !== undefined) window.clearTimeout(this.#timer);
    this.element.remove();
  }
}
