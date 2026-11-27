// Turns the bot's QR text into an SVG built node by node: no `innerHTML`, nothing the CSP blocks.
import qrcode from "qrcode-generator";

const SVG = "http://www.w3.org/2000/svg";
/** The empty border, in modules, that scanners need. */
const QUIET_ZONE = 4;

export interface QrDrawing {
  /** Side of the square, in modules, quiet zone included. */
  readonly side: number;
  /** SVG path with one square per dark module. */
  readonly path: string;
}

export function qrDrawing(text: string): QrDrawing {
  const qr = qrcode(0, "M");
  qr.addData(text);
  qr.make();
  const count = qr.getModuleCount();
  let path = "";
  for (let row = 0; row < count; row += 1) {
    for (let column = 0; column < count; column += 1) {
      if (qr.isDark(row, column)) path += `M${column + QUIET_ZONE} ${row + QUIET_ZONE}h1v1h-1z`;
    }
  }
  return { side: count + 2 * QUIET_ZONE, path };
}

export function qrImage(text: string): SVGSVGElement {
  const { side, path } = qrDrawing(text);
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", `0 0 ${side} ${side}`);
  svg.setAttribute("class", "bot-qr-image");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "Código QR para vincular el WhatsApp del bot");
  svg.setAttribute("shape-rendering", "crispEdges");
  // Always dark on white, whatever the theme, so it can be scanned.
  const background = document.createElementNS(SVG, "rect");
  background.setAttribute("width", String(side));
  background.setAttribute("height", String(side));
  background.setAttribute("fill", "#ffffff");
  const modules = document.createElementNS(SVG, "path");
  modules.setAttribute("d", path);
  modules.setAttribute("fill", "#000000");
  svg.append(background, modules);
  return svg;
}
