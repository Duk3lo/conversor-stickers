import { normalize } from "./catalog.ts";
import { EMOJIS_GROUPS } from "./emoji-list.ts";

export const RECENT_MAX = 16;
export const MAX_EMOJIS_STICKER = 3;
const RECENT_KEY = "emojis-recent";

export interface EmojiEntry {
  readonly emoji: string;
  readonly words: string;
}

export interface GroupOfEmojis {
  readonly id: string;
  readonly name: string;
  readonly emojis: readonly EmojiEntry[];
}

export const Groups: readonly GroupOfEmojis[] = EMOJIS_GROUPS.map((group) => ({
  id: group.id,
  name: group.name,
  emojis: group.lines.map((line) => {
    const [emoji = "", ...words] = line.split(" ");
    return { emoji, words: normalize(words.join(" ")) };
  }),
}));

export function searchEmojis(query: string, groups: readonly GroupOfEmojis[] = Groups): EmojiEntry[] {
  const words = normalize(query).split(/\s+/).filter((word) => word !== "");
  if (words.length === 0) return [];
  const seen = new Set<string>();
  const result: EmojiEntry[] = [];
  for (const entry of groups.flatMap((group) => group.emojis)) {
    if (seen.has(entry.emoji)) continue;
    if (words.every((word) => entry.words.includes(word) || entry.emoji.startsWith(word))) {
      seen.add(entry.emoji);
      result.push(entry);
    }
  }
  return result;
}

export function withRecent(recent: readonly string[], emoji: string): string[] {
  return [emoji, ...recent.filter((current) => current !== emoji)].slice(0, RECENT_MAX);
}

type Store = Pick<Storage, "getItem" | "setItem">;

export function readRecent(store: Store | null): string[] {
  try {
    const text = store?.getItem(RECENT_KEY) ?? null;
    if (text === null) return [];
    const value: unknown = JSON.parse(text);
    if (!Array.isArray(value)) return [];
    return value.filter((element): element is string => typeof element === "string" && element !== "").slice(0, RECENT_MAX);
  } catch {
    return [];
  }
}

export function saveRecent(store: Store | null, recent: readonly string[]): boolean {
  try {
    store?.setItem(RECENT_KEY, JSON.stringify(recent.slice(0, RECENT_MAX)));
    return store !== null;
  } catch {
    return false;
  }
}

export interface Selection {
  readonly emojis: readonly string[];
  readonly notice: string | null;
}

export function toggleEmoji(current: readonly string[], emoji: string): Selection {
  if (current.includes(emoji)) {
    if (current.length === 1) return { emojis: current, notice: "Un sticker necesita al menos un emoji." };
    return { emojis: current.filter((current) => current !== emoji), notice: null };
  }
  if (current.length >= MAX_EMOJIS_STICKER) {
    return { emojis: current, notice: `Un sticker lleva como mucho ${MAX_EMOJIS_STICKER} emojis.` };
  }
  return { emojis: [...current, emoji], notice: null };
}
