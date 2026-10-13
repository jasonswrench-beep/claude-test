// SPDX-License-Identifier: Apache-2.0
/**
 * Colours, and whether text can be read against its background (story C7; WCAG 2.2 success criterion 1.4.3).
 *
 * Colours are written "#RRGGBB". The contrast ratio is the one WCAG defines: the relative brightness
 * of the lighter colour plus 0.05, divided by that of the darker plus 0.05. Ordinary text needs 4.5 to 1;
 * large text needs 3 to 1.
 */

export const NORMAL_TEXT_RATIO = 4.5;
export const LARGE_TEXT_RATIO = 3;

/** Text of this height or more (as ten-thousandths of the picture's height) counts as large: 24 pixels, or 18.7 bold, on a picture 1080 pixels high. */
export const LARGE_TEXT_BP = 222;
export const LARGE_BOLD_TEXT_BP = 173;
/** Text smaller than this is very small and gets a warning: about 16 pixels on a picture 1080 pixels high. */
export const VERY_SMALL_TEXT_BP = 150;

const COLOR = /^#[0-9a-f]{6}$/i;

export function isColor(value: string): boolean {
  return COLOR.test(value);
}

/** A colour in one fixed form (capital letters), or undefined if it is not a colour. */
export function normalizeColor(value: string): string | undefined {
  const trimmed = value.trim();
  const withHash = trimmed.startsWith("#") ? trimmed : `#${trimmed}`;
  return isColor(withHash) ? withHash.toUpperCase() : undefined;
}

function channel(hex: string, start: number): number {
  const v = parseInt(hex.slice(start, start + 2), 16) / 255;
  return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** How bright a colour is, from 0 (black) to 1 (white), as WCAG measures it. */
export function relativeLuminance(color: string): number {
  return 0.2126 * channel(color, 1) + 0.7152 * channel(color, 3) + 0.0722 * channel(color, 5);
}

/** The contrast ratio between two colours, from 1 (the same) to 21 (black on white). */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const [light, dark] = la >= lb ? [la, lb] : [lb, la];
  return (light + 0.05) / (dark + 0.05);
}

/** Whether text of this size and weight counts as large for contrast. */
export function isLargeText(fontSizeBp: number, bold: boolean): boolean {
  return fontSizeBp >= (bold ? LARGE_BOLD_TEXT_BP : LARGE_TEXT_BP);
}

/** The ratio that text of this size and weight needs. */
export function neededRatio(fontSizeBp: number, bold: boolean): number {
  return isLargeText(fontSizeBp, bold) ? LARGE_TEXT_RATIO : NORMAL_TEXT_RATIO;
}

/** The ratio in words, such as "2.1 to 1". */
export function describeRatio(ratio: number): string {
  return `${(Math.floor(ratio * 10) / 10).toFixed(1)} to 1`;
}
