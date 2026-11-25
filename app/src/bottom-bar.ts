import type { PanelVisible } from "./navigation.ts";
import type { Platform } from "./mobile.ts";

export type BottomAction = "fab-create" | "editor-create" | "editor-result" | "pack-bar" | "none";

export function bottomAction(platform: Platform, panel: PanelVisible, resultReady: boolean): BottomAction {
  if (platform !== "android") return "none";
  switch (panel) {
    case "start":
    case "mis-packs":
      return "fab-create";
    case "editor":
      return resultReady ? "editor-result" : "editor-create";
    case "detail":
      return "pack-bar";
    default:
      return "none";
  }
}
