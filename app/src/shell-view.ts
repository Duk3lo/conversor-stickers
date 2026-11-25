import { element, showMessage } from "./dom.ts";
import { bottomAction } from "./bottom-bar.ts";
import { controls, destinationLabel, tabLabel } from "./mobile.ts";
import {
  type Navigation,
  type PanelVisible,
  panelVisible,
  Roots,
  activeRoot,
  Sections,
  type Section,
  sectionWithKey,
} from "./navigation.ts";
import { PLAY_LOGO_COLORS, play } from "./edition.ts";
import { platform } from "./platform.ts";
import { folderButtonVisible, type SavedFolder } from "./saved-folder.ts";
import type { ShellActions, ShellView } from "./views.ts";

const Panels: readonly PanelVisible[] = [...Roots, "detail", "category", "search", "editor"];

function paintPlayLogo(): void {
  for (const part of document.querySelectorAll<SVGElement>(".header-logo [data-logo]")) {
    const color = PLAY_LOGO_COLORS[part.dataset["logo"] ?? ""];
    if (color === undefined) continue;
    const attribute = part.tagName === "stop" ? "stop-color" : part.hasAttribute("stroke") ? "stroke" : "fill";
    part.setAttribute(attribute, color);
  }
}

function idPanel(panel: PanelVisible): string {
  return panel === "editor" ? "editor" : `panel-${panel}`;
}

export function createShellView(actions: ShellActions): { render(view: ShellView): void } {
  const destinations = new Map(Roots.map((root) => [root, element<HTMLButtonElement>(`destination-${root}`)] as const));
  const panels = new Map(Panels.map((panel) => [panel, element<HTMLElement>(idPanel(panel))] as const));
  const tabs = new Map<Section, HTMLButtonElement>();
  const subpanels = new Map<Section, HTMLElement>();
  for (const section of Sections) {
    tabs.set(section, element<HTMLButtonElement>(`tab-${section}`));
    subpanels.set(section, element<HTMLElement>(`panel-${section}`));
  }
  const notice = element<HTMLDivElement>("notice-global");
  const noticeText = element<HTMLParagraphElement>("notice-global-text");
  const closeNotice = element<HTMLButtonElement>("notice-global-close");
  const noticeFolder = element<HTMLButtonElement>("notice-global-folder");
  let folder: SavedFolder | null = null;
  noticeFolder.addEventListener("click", () => {
    if (folder !== null) actions.openFolder(folder);
  });
  const editor = panels.get("editor");
  const fab = element<HTMLButtonElement>("fab-create");
  fab.addEventListener("click", () => actions.goToCreate());
  let scrollTimer: number | undefined;
  window.addEventListener(
    "scroll",
    () => {
      fab.dataset["scrolling"] = "true";
      window.clearTimeout(scrollTimer);
      scrollTimer = window.setTimeout(() => delete fab.dataset["scrolling"], 250);
    },
    { passive: true },
  );

  let previous: Navigation | null = null;

  document.documentElement.dataset["platform"] = platform;
  if (play) {
    document.documentElement.dataset["edition"] = "play";
    paintPlayLogo();
  }
  for (const [root, button] of destinations) {
    const name = destinationLabel(platform, root);
    element<HTMLSpanElement>(`destination-${root}-text`).textContent = name;
    button.setAttribute("aria-label", destinationLabel("desktop", root));
    button.addEventListener("click", () => actions.goToRoot(root));
  }
  const label = tabLabel("download", play);
  element<HTMLSpanElement>("tab-download-name").textContent = label.name;
  element<HTMLSpanElement>("tab-download-detail").textContent = label.detail;
  element<HTMLElement>("credits").hidden = !controls(platform).ffmpegCredits;

  for (const [section, tab] of tabs) {
    tab.addEventListener("click", () => actions.changeSection(section));
    tab.addEventListener("keydown", (event) => {
      const destination = sectionWithKey(section, event.key);
      if (destination === null) return;
      event.preventDefault();
      actions.changeSection(destination);
      tabs.get(destination)?.focus();
    });
  }
  closeNotice.addEventListener("click", () => actions.closeNotice());

  function moveFocus(nav: Navigation): void {
    if (previous === null) return;
    const now = panelVisible(nav);
    const packChange = now === "mis-packs" && nav.openedPack !== previous.openedPack;
    if (now === panelVisible(previous) && !packChange) return;
    window.scrollTo({ top: 0 });
    panels.get(now)?.focus({ preventScroll: true });
  }

  return {
    render(view) {
      const { nav } = view;
      const visible = panelVisible(nav);
      const active = activeRoot(nav);
      for (const [root, button] of destinations) {
        if (root === active) button.setAttribute("aria-current", "page");
        else button.removeAttribute("aria-current");
      }
      for (const [panel, node] of panels) node.hidden = panel !== visible;
      for (const [section, tab] of tabs) {
        const chosen = section === nav.section;
        tab.setAttribute("aria-selected", String(chosen));
        tab.tabIndex = chosen ? 0 : -1;
        const subpanel = subpanels.get(section);
        if (subpanel) subpanel.hidden = !chosen;
      }
      fab.hidden = bottomAction(platform, visible, false) !== "fab-create";
      if (editor) editor.dataset["destination"] = nav.editor?.destination ?? "";
      showMessage(noticeText, view.notice);
      notice.hidden = view.notice === null;
      folder = view.noticeFolder;
      noticeFolder.hidden = !folderButtonVisible(platform, folder);
      moveFocus(nav);
      previous = nav;
    },
  };
}
