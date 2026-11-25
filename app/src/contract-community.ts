import { Channel } from "@tauri-apps/api/core";

import { call, type CreatedPack } from "./contract.ts";

export interface CommunityCategoryUi {
  id: string;
  name: string;
}

export interface CatalogUi {
  categories: CommunityCategoryUi[];
  packs: CommunityPackUi[];
  updated: number;
  offline: boolean;
  notice_accepted: boolean;
}

export interface CommunityPackUi {
  id: string;
  version: number;
  name: string;
  author: string;
  license: string;
  category: string;
  tags: string[];
  animated: boolean;
  added: string;
  stickers: number;
  icon: string | null;
  downloaded: number | null;
  updatable: boolean;
}

export interface CommunitySampleUi {
  path: string;
  emojis: string[];
}

export interface CommunityDetailUi {
  pack: CommunityPackUi;
  stickers: CommunitySampleUi[];
}

export type CommunityProgressUi = { kind: "downloading"; done: number; total: number } | { kind: "verifying" };

export function catalog(force: boolean): Promise<CatalogUi> {
  return call("catalog", { force });
}

export function communityDetail(id: string): Promise<CommunityDetailUi> {
  return call("community_detail", { id });
}

export function acceptCommunityNotice(): Promise<void> {
  return call("accept_community_notice");
}

export function downloadFromCommunity(id: string, onProgress: (progress: CommunityProgressUi) => void): Promise<CreatedPack> {
  const channel = new Channel<CommunityProgressUi>();
  channel.onmessage = onProgress;
  return call("download_from_community", { id, channel });
}

export function cancelCommunityDownload(): Promise<void> {
  return call("cancel_community_download");
}

export function proposeToCommunity(pack: number, license: string, category: string): Promise<string | null> {
  return call("propose_to_community", { pack, license, category });
}
