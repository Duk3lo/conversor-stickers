import assert from "node:assert/strict";
import { test } from "node:test";

import {
  openEditor,
  openPack,
  back,
  changeSection,
  closeEditor,
  withPacks,
  goTo,
  goToRoot,
  type Navigation,
  initialNavigation,
  panelVisible,
  activeRoot,
  replace,
  Roots,
  type Path,
  pathToText,
  sectionWithKey,
  withoutEntry,
  textToPath,
} from "./navigation.ts";

const initial = initialNavigation();
const localDetail: Path = { kind: "pack", origin: "local", id: "4" };
const communityDetail: Path = { kind: "pack", origin: "community", id: "gatos-garabato" };

test("the app starts at Home, with no stack, editor or open pack", () => {
  assert.deepEqual(initial, {
    path: { kind: "start" },
    stack: [],
    section: "stickers",
    editor: null,
    openedPack: null,
  });
});

test("routes round trip to text, with the formats in the plan", () => {
  const cases: [Path, string][] = [
    [{ kind: "start" }, "start"],
    [{ kind: "mis-packs" }, "mis-packs"],
    [localDetail, "pack/local/4"],
    [communityDetail, "pack/community/gatos-garabato"],
    [{ kind: "category", origin: "community", id: "animales" }, "category/community/animales"],
    [{ kind: "search", query: "gato/perro" }, "search/gato/perro"],
  ];
  for (const [path, text] of cases) {
    assert.equal(pathToText(path), text);
    assert.deepEqual(textToPath(text), path);
  }
});

test("a text that is not a path gives null", () => {
  for (const text of ["", "otra", "pack", "pack/web/3", "pack/local/", "category/local"]) {
    assert.equal(textToPath(text), null, text);
  }
});

test("going to a bar destination empties the stack and closes the editor", () => {
  const inside = openEditor(goTo(initial, communityDetail), 3, "sticker");
  const nav = goToRoot(inside, "favorites");
  assert.deepEqual(nav.path, { kind: "favorites" });
  assert.deepEqual(nav.stack, []);
  assert.equal(nav.editor, null);
});

test("the detail is stacked over the origin route and back returns to it", () => {
  const open = goTo(goToRoot(initial, "community"), communityDetail);
  assert.deepEqual(open.path, communityDetail);
  assert.deepEqual(open.stack, [{ kind: "community" }]);
  const returned = back(open);
  assert.deepEqual(returned?.path, { kind: "community" });
  assert.deepEqual(returned?.stack, []);
  assert.equal(back(returned ?? initial), null);
});

test("the stack goes through several routes: Home, category, detail", () => {
  const category: Path = { kind: "category", origin: "community", id: "animales" };
  const nav = goTo(goTo(initial, category), communityDetail);
  assert.deepEqual(nav.stack.map(pathToText), ["start", "category/community/animales"]);
  assert.deepEqual(back(nav)?.path, category);
});

test("entering the same route twice does not stack", () => {
  const una = goTo(initial, localDetail);
  assert.equal(goTo(una, localDetail), una);
});

test("the stack has a cap and forgets the oldest", () => {
  let nav = initial;
  for (let i = 0; i < 40; i++) nav = goTo(nav, { kind: "search", query: String(i) });
  assert.equal(nav.stack.length, 20);
  assert.deepEqual(nav.stack[19], { kind: "search", query: "38" });
});

test("the editor opens over the route and back closes it first", () => {
  const inDetail = goTo(initial, localDetail);
  const editing = openEditor(inDetail, 9, "sticker");
  assert.deepEqual(back(editing), inDetail);
  assert.equal(closeEditor(inDetail), inDetail);
});

test("back closes the open own pack before the stack", () => {
  const open = openPack(initial, 4);
  assert.deepEqual(open.path, { kind: "mis-packs" });
  assert.equal(open.openedPack, 4);
  assert.deepEqual(back(open), { ...open, openedPack: null });
});

test("at a root destination back does nothing: it lets the app exit", () => {
  for (const root of Roots) assert.equal(back(goToRoot(initial, root)), null, root);
});

test("switching subtab goes to Create, closes the editor and is kept on return", () => {
  const editing = openEditor(initial, 9, "sticker");
  const inGifs = changeSection(editing, "gifs");
  assert.deepEqual(inGifs.path, { kind: "create" });
  assert.equal(inGifs.section, "gifs");
  assert.equal(inGifs.editor, null);
  assert.equal(goToRoot(goToRoot(inGifs, "start"), "create").section, "gifs");
  assert.equal(changeSection(editing, "stickers").editor, null);
});

test("a deleted own pack closes the editing and the detail", () => {
  const open = openPack(initial, 4);
  assert.equal(withPacks(open, [1, 2]).openedPack, null);
  assert.equal(withPacks(open, [4]), open);
  const detail = goTo(goToRoot(initial, "favorites"), localDetail);
  assert.deepEqual(withPacks(detail, [1]).path, { kind: "favorites" });
  assert.equal(withPacks(detail, [4]), detail);
  assert.equal(withPacks(goTo(initial, communityDetail), []).path, communityDetail);
  assert.equal(withPacks(initial, []), initial);
});

test("removing the editor entry closes it; another entry does not", () => {
  const editing = openEditor(changeSection(initial, "download"), 5, "gif");
  assert.equal(withoutEntry(editing, 5).editor, null);
  assert.equal(withoutEntry(editing, 6), editing);
});

test("the bar marks the origin destination of a detail route", () => {
  assert.equal(activeRoot(initial), "start");
  assert.equal(activeRoot(goTo(goToRoot(initial, "community"), communityDetail)), "community");
  assert.equal(activeRoot(goTo(goTo(goToRoot(initial, "favorites"), localDetail), { kind: "search", query: "a" })), "favorites");
  assert.equal(activeRoot({ ...initial, path: localDetail }), "start");
});

test("there is only one visible panel and the editor covers the rest", () => {
  assert.equal(panelVisible(initial), "start");
  assert.equal(panelVisible(goTo(initial, localDetail)), "detail");
  assert.equal(panelVisible(goTo(initial, { kind: "search", query: "" })), "search");
  assert.equal(panelVisible(goTo(initial, { kind: "category", origin: "local", id: "1" })), "category");
  assert.equal(panelVisible(openEditor(goToRoot(initial, "create"), 1, "gif")), "editor");
});

test("the arrows cycle through the Create subtabs; Home and End go to the ends", () => {
  const cases: [Navigation["section"], string, Navigation["section"] | null][] = [
    ["stickers", "ArrowRight", "gifs"],
    ["gifs", "ArrowRight", "download"],
    ["download", "ArrowRight", "search-gif"],
    ["search-gif", "ArrowRight", "stickers"],
    ["stickers", "ArrowLeft", "search-gif"],
    ["gifs", "ArrowLeft", "stickers"],
    ["download", "Home", "stickers"],
    ["stickers", "End", "search-gif"],
    ["gifs", "Enter", null],
    ["gifs", "ArrowDown", null],
  ];
  for (const [from, key, expected] of cases) {
    assert.equal(sectionWithKey(from, key), expected, `${from} + ${key}`);
  }
});

test("replace changes the route without stacking, to refine a search", () => {
  const searching = goTo(initial, { kind: "search", query: "gato" });
  const refined = replace(searching, { kind: "search", query: "gato negro" });
  assert.deepEqual(refined.path, { kind: "search", query: "gato negro" });
  assert.deepEqual(refined.stack, searching.stack);
  assert.deepEqual(back(refined)?.path, { kind: "start" });
});
