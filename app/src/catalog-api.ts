import * as commands from "./contract.ts";
import type { LibraryUi, PackEdit, FavoriteTarget, CreatedPack } from "./contract.ts";
import * as community from "./contract-community.ts";
import type { CatalogUi, CommunityDetailUi, CommunityProgressUi } from "./contract-community.ts";
import * as mobile from "./contract-mobile.ts";
import type { WhatsappOutcome } from "./contract-mobile.ts";

export interface CatalogApi {
  setEmojis(pack: number, sticker: number, emojis: string[]): Promise<LibraryUi>;
  markFavorite(target: FavoriteTarget, favorite: boolean): Promise<LibraryUi>;
  reorderStickers(pack: number, order: number[]): Promise<LibraryUi>;
  createPackFromFavorites(name: string): Promise<CreatedPack>;
  exportBackup(): Promise<string | null>;
  restoreBackup(): Promise<LibraryUi | null>;
  editPack(id: number, edit: PackEdit): Promise<LibraryUi>;
  deletePack(id: number): Promise<LibraryUi>;
  removeSticker(pack: number, sticker: number): Promise<LibraryUi>;
  exportPack(id: number): Promise<string | null>;
  exportSticker(pack: number, sticker: number): Promise<string | null>;
  shareSticker(pack: number, sticker: number): Promise<void>;
  sendStickerToWhatsapp(pack: number, sticker: number): Promise<WhatsappOutcome>;
  addToWhatsapp(pack: number): Promise<WhatsappOutcome>;
  catalog(force: boolean): Promise<CatalogUi>;
  communityDetail(id: string): Promise<CommunityDetailUi>;
  acceptCommunityNotice(): Promise<void>;
  downloadFromCommunity(id: string, onProgress: (progress: CommunityProgressUi) => void): Promise<CreatedPack>;
  cancelCommunityDownload(): Promise<void>;
  proposeToCommunity(pack: number, license: string, category: string): Promise<string | null>;
}

export const apiReal: CatalogApi = {
  setEmojis: commands.setEmojis,
  markFavorite: commands.markFavorite,
  reorderStickers: commands.reorderStickers,
  createPackFromFavorites: commands.createPackFromFavorites,
  exportBackup: commands.exportBackup,
  restoreBackup: commands.restoreBackup,
  editPack: commands.editPack,
  deletePack: commands.deletePack,
  removeSticker: commands.removeSticker,
  exportPack: commands.exportPack,
  exportSticker: commands.exportSticker,
  shareSticker: mobile.shareSticker,
  sendStickerToWhatsapp: mobile.sendStickerToWhatsapp,
  addToWhatsapp: mobile.addToWhatsapp,
  catalog: community.catalog,
  communityDetail: community.communityDetail,
  acceptCommunityNotice: community.acceptCommunityNotice,
  downloadFromCommunity: community.downloadFromCommunity,
  cancelCommunityDownload: community.cancelCommunityDownload,
  proposeToCommunity: community.proposeToCommunity,
};
