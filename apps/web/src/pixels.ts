/** The panel's data language: fields of pixels lit up to a value and dithered past it (the effort
 * field, the usage bars). One rule for all of them. */

/** Columns past the edge where the light breaks into dither, like a glow fading out. */
const EDGE = 4
/** Ordered 4×4 dither thresholds, 0 to 1. */
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5].map((n) => (n + 0.5) / 16)

/** A cell's light, 0 to 1: lit up to `edge` (brighter toward it), dithered for a few columns past it. */
export function lit(col: number, row: number, edge: number): number {
  if (col < edge) return 0.35 + 0.65 * (col / Math.max(1, edge))
  const past = col - edge
  if (past >= EDGE) return 0
  return BAYER[(row % 4) * 4 + (col % 4)] > (past + 1) / (EDGE + 1) ? 0.3 : 0
}
