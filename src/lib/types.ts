// Mirrors of the serde structs sent over IPC (camelCase on the wire).

export interface DemoFile {
  path: string;
  size: number;
}

export interface DemoList {
  files: DemoFile[];
  totalBytes: number;
}

export type Phase = 'discover' | 'run';

export interface Progress {
  phase: Phase;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
  cacheHits: number;
  elapsedMs: number;
  finished: boolean;
}

export interface PlayerHit {
  name: string;
  demos: number;
}

export interface DiscoverResult {
  players: PlayerHit[];
  scanned: number;
  fromCache: number;
  errors: string[];
  cancelled: boolean;
  elapsedMs: number;
}

export type Condition = 'AND' | 'OR';

export type RuleKind =
  | 'damage'
  | 'frags'
  | 'speed'
  | 'accuracy'
  | 'siphonator'
  | 'flagCarrier'
  | 'roundWin'
  | 'falloutDeath';

interface RuleBase {
  id: string;
  kind: RuleKind;
}

export type SearchRule =
  | (RuleBase & { kind: 'damage'; minimum: number; weapon: number | null })
  | (RuleBase & { kind: 'frags'; minimum: number; weapon: number | null })
  | (RuleBase & { kind: 'speed'; minimum: number; duration: number })
  | (RuleBase & { kind: 'accuracy'; minimum: number; weapon: number })
  | (RuleBase & { kind: 'falloutDeath'; exclude: boolean })
  | (RuleBase & { kind: 'siphonator' | 'flagCarrier' | 'roundWin' });

export interface RuleGroup {
  id: string;
  condition: Condition;
  rules: SearchRule[];
}

export interface RuleQuery {
  condition: Condition;
  groups: RuleGroup[];
}

export interface Params {
  players: string[];
  damage: number | null;
  frags: number | null;
  weapon: number | null;
  speed: number | null;
  speedDuration: number | null;
  accuracy: number | null;
  condition: Condition;
  win: boolean;
  window: number | null;
  ruleQuery: RuleQuery | null;
}

export interface DemoMeta {
  magic: 'EVGR' | 'DBSR';
  formatVersion: number;
  appVersion: string;
  gameMode: string;
  mapName: string;
  createdAt: number | null;
  timeElapsed: number | null;
  agents: string[];
}

export interface Scene {
  start: number;
  clock: string;
  player: string;
  damage: number | null;
  frags: number | null;
  weapon: number | null;
  speed: number | null;
  speedDuration: number | null;
  accuracy: number | null;
  round: number | null;
  criteria: SceneMetric[];
  line: string;
}

export interface SceneMetric {
  ruleId: string;
  label: string;
  value: number;
  secondary: number | null;
  display: string;
}

export interface DemoReport {
  path: string;
  fileName: string;
  meta: DemoMeta | null;
  error: string | null;
  sceneCount: number | null;
  scenes: Scene[];
  missingPlayers: string[];
  playersSeen: string[];
  truncated: boolean;
  stoppedEarly: boolean;
  fromCache: boolean;
  scanMs: number;
  outLines: string[];
  errLines: string[];
}

export interface RunReport {
  headerLine: string;
  demos: DemoReport[];
  demoCount: number;
  demosWithPlayer: number;
  totalScenes: number;
  bulk: boolean;
  cancelled: boolean;
  cacheHits: number;
  elapsedMs: number;
  text: string;
}

export interface CacheStats {
  entries: number;
  bytes: number;
}
