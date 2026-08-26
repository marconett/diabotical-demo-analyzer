export const weapons = [
  { id: 0, name: 'Melee' },
  { id: 1, name: 'Machine Gun' },
  { id: 2, name: 'Blaster' },
  { id: 3, name: 'Super Shotgun' },
  { id: 4, name: 'Rocket Launcher' },
  { id: 5, name: 'Shaft' },
  { id: 6, name: 'Crossbow' },
  { id: 7, name: 'PnCR' },
  { id: 8, name: 'Grenade Launcher' },
  { id: 12, name: 'Healing Weeball' },
  { id: 13, name: 'Implosion Weeball' },
  { id: 14, name: 'Slowfield Weeball' },
  { id: 15, name: 'Explosive Weeball' },
  { id: 16, name: 'Smoke Weeball' },
  { id: 17, name: 'Knock Weeball' },
  { id: 18, name: 'Hook' },
  { id: 19, name: 'Void Cannon' },
  { id: 20, name: 'Super Shotgun Secondary' },
  { id: 21, name: 'Rocket Launcher Secondary' },
  { id: 22, name: 'Void Cannon Secondary' },
  { id: 23, name: 'Survival Amplifier' },
  { id: 24, name: 'Survival Vulnerability' },
  { id: 25, name: 'Survival Heal' },
  { id: 205, name: 'Ring Out' },
] as const;

export function weaponName(id: number | null): string {
  return id === null ? '' : (weapons.find((weapon) => weapon.id === id)?.name ?? `Weapon ${id}`);
}
