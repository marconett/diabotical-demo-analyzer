import { Channel, invoke } from '@tauri-apps/api/core';
import type { CacheStats, DemoList, DiscoverResult, Params, Progress, RunReport } from './types';

export function listDemos(paths: string[]): Promise<DemoList> {
  return invoke('list_demos', { paths });
}

export function discoverPlayers(
  paths: string[],
  onProgress: (p: Progress) => void,
): Promise<DiscoverResult> {
  const channel = new Channel<Progress>();
  channel.onmessage = onProgress;
  return invoke('discover_players', { paths, onProgress: channel });
}

export function runSceneFinder(
  paths: string[],
  params: Params,
  onProgress: (p: Progress) => void,
): Promise<RunReport> {
  const channel = new Channel<Progress>();
  channel.onmessage = onProgress;
  return invoke('run_scene_finder', { paths, params, onProgress: channel });
}

export function cancel(): Promise<void> {
  return invoke('cancel');
}

export function cacheStats(): Promise<CacheStats> {
  return invoke('cache_stats');
}

export function clearCache(): Promise<CacheStats> {
  return invoke('clear_cache');
}
