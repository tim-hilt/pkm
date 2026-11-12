/**
 * Where a Gallery card's menu opens: its corner at the press `at` (a viewport
 * point), kept within the `card` (its viewport box), in the card's coordinates.
 */
export function menuCorner(
  at: { x: number; y: number },
  card: { left: number; top: number; width: number; height: number },
  menu: { width: number; height: number },
): { left: number; top: number } {
  return {
    left: Math.max(0, Math.min(at.x - card.left, card.width - menu.width)),
    top: Math.max(0, Math.min(at.y - card.top, card.height - menu.height)),
  };
}
