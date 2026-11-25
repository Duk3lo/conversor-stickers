export function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (found === null) throw new Error(`falta #${id} en index.html`);
  return found as T;
}

const SVG = "http://www.w3.org/2000/svg";

export function icon(name: string, className = "icon"): SVGSVGElement {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", className);
  svg.setAttribute("aria-hidden", "true");
  const use = document.createElementNS(SVG, "use");
  use.setAttribute("href", `#icon-${name}`);
  svg.append(use);
  return svg;
}

export function showMessage(node: HTMLElement, text: string | null): void {
  node.hidden = text === null;
  node.textContent = text ?? "";
}

export function create<K extends keyof HTMLElementTagNameMap>(
  label: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(label);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function pressedButton(event: Event): HTMLButtonElement | null {
  const target = event.target instanceof Element ? event.target : null;
  const button = target?.closest("button") ?? null;
  return button !== null && !button.disabled ? button : null;
}

export function button(className: string, text: string, iconName: string | null, label?: string): HTMLButtonElement {
  const node = document.createElement("button");
  node.type = "button";
  node.className = className;
  if (iconName !== null) node.append(icon(iconName));
  if (text !== "") node.append(document.createTextNode(text));
  if (label !== undefined) node.setAttribute("aria-label", label);
  return node;
}

export function renderFocusKeeping(container: HTMLElement, render: () => void): void {
  const active = document.activeElement;
  const mark = active instanceof HTMLElement && container.contains(active) ? (active.dataset["focus"] ?? null) : null;
  render();
  if (mark === null) return;
  for (const candidate of container.querySelectorAll<HTMLElement>("[data-focus]")) {
    if (candidate.dataset["focus"] === mark) {
      candidate.focus({ preventScroll: true });
      return;
    }
  }
}

export function image(url: string | null, className: string): HTMLElement {
  if (url === null) return create("span", `${className} sin-imagen`);
  const node = document.createElement("img");
  node.className = className;
  node.src = url;
  node.alt = "";
  node.loading = "lazy";
  node.decoding = "async";
  return node;
}
