export type Store = Pick<Storage, "getItem" | "setItem">;

export function localStore(): Store | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
