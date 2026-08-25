import type { Params } from './types';

/** Same checks and messages as the Rust side, for instant inline feedback. */
export function validateParams(p: Params): string | null {
  if (p.players.length === 0) return 'at least one player name is required';
  const hasThreshold = p.damage !== null || p.frags !== null;
  if (!hasThreshold && !p.win)
    return 'at least one of -dmg/--damage_threshold, -frags or -win is required';
  if (p.window === null && hasThreshold) return '-t/--time_window is required with -dmg/-frags';
  if (p.window !== null && !(p.window > 0)) return '--time_window must be > 0';
  return null;
}
