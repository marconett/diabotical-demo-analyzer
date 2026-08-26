import type { Params } from './types';

/** Same checks and messages as the Rust side, for instant inline feedback. */
export function validateParams(p: Params): string | null {
  if (p.players.length === 0) return 'at least one player name is required';
  if (p.ruleQuery) {
    if (p.ruleQuery.groups.length === 0 || p.ruleQuery.groups.some((group) => !group.rules.length))
      return 'add at least one criterion to every group';
    const ids = p.ruleQuery.groups.flatMap((group) => group.rules.map((rule) => rule.id));
    if (ids.some((id) => !id) || new Set(ids).size !== ids.length)
      return 'criteria must have unique ids';
    for (const rule of p.ruleQuery.groups.flatMap((group) => group.rules)) {
      if (rule.kind === 'speed' && !(rule.duration >= 0)) return 'speed duration must be >= 0';
      if (rule.kind === 'accuracy' && !(rule.minimum >= 0 && rule.minimum <= 100))
        return 'reported accuracy must be between 0 and 100';
      if ('minimum' in rule && !(rule.minimum >= 0)) return 'minimum values must be >= 0';
    }
    if (p.window === null) return 'a time window is required';
    if (!(p.window > 0)) return 'time window must be > 0';
    return null;
  }
  const hasThreshold =
    p.damage !== null || p.frags !== null || p.speed !== null || p.accuracy !== null;
  if (!hasThreshold && !p.win)
    return 'at least one damage, frag, speed, accuracy or round-win criterion is required';
  if (p.window === null && hasThreshold)
    return 'a time window is required with damage/frags/speed/accuracy';
  if (p.window !== null && !(p.window > 0)) return '--time_window must be > 0';
  if (p.speedDuration !== null && p.speed === null)
    return 'speed duration requires a minimum speed';
  if (p.speedDuration !== null && !(p.speedDuration >= 0)) return 'speed duration must be >= 0';
  if (p.accuracy !== null && p.weapon === null) return 'reported accuracy requires a weapon';
  if (p.accuracy !== null && !(p.accuracy >= 0 && p.accuracy <= 100))
    return 'reported accuracy must be between 0 and 100';
  return null;
}
