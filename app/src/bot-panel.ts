// The «Mi bot» panel of the bot extension card. It is built once and only its texts and visibility
// change, so asking the bot every few seconds moves neither the focus nor the screen reader.
import type { BotLinkMethod } from "./contract-bot.ts";
import { asErrorUi } from "./contract.ts";
import type { BotMonitor } from "./bot-monitor.ts";
import { type BotState, botActions, botEnabled, botNotice, describeBot, phoneFieldVisible, qrNoteText, shouldConnectLocal, submitLink } from "./bot-status.ts";
import { create, showMessage } from "./dom.ts";
import { errorMessage } from "./messages.ts";
import type { Platform } from "./mobile.ts";
import { qrImage } from "./bot-qr.ts";

export const BOT_PANEL_ID = "bot-panel";

export interface BotPanelDependencies {
  readonly monitor: BotMonitor;
  readonly platform: Platform;
  openWhatsapp(): void;
}

/**
 * Shows the panel over the editor, which keeps its range and settings: the panel node moves into a
 * dialog and goes back to its card when the dialog closes.
 */
export function showBotPanelDialog(): void {
  const panel = document.getElementById(BOT_PANEL_ID);
  if (panel === null || panel.closest("dialog") !== null) return;
  const home = document.createComment("bot-panel");
  panel.replaceWith(home);
  const dialog = create("dialog", "bot-dialog");
  dialog.setAttribute("aria-labelledby", "bot-panel-title");
  const close = button("button-secondary", "Volver al editor");
  close.addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => {
    home.replaceWith(panel);
    dialog.remove();
  });
  dialog.append(panel, close);
  document.body.append(dialog);
  dialog.showModal();
  panel.focus();
}

export function closeBotPanelDialog(): void {
  document.querySelector<HTMLDialogElement>("dialog.bot-dialog")?.close();
}

/** Tells the monitor while `node` is on screen: hidden views must not keep asking the bot. */
export function watchVisibility(monitor: BotMonitor, key: string, node: Element): void {
  new IntersectionObserver((entries) => {
    const last = entries[entries.length - 1];
    if (last !== undefined) monitor.watch(key, last.isIntersecting);
  }).observe(node);
}

export function linkSteps(method: BotLinkMethod): readonly string[] {
  const common = [
    "Abre el WhatsApp que vas a vincular. Lo más seguro es un número secundario (puede ser WhatsApp Business o estar en otro teléfono).",
    "Entra en «Dispositivos vinculados» (en Android, menú ⋮; en iPhone, Ajustes) y pulsa «Vincular un dispositivo».",
  ];
  return method === "code"
    ? [...common, "Pulsa «Vincular con número de teléfono» y escribe el código."]
    : [...common, "Escanea con la cámara el código QR de esta pantalla."];
}

function button(className: string, text: string): HTMLButtonElement {
  const node = create("button", className, text);
  node.type = "button";
  return node;
}

function field(id: string, text: string, type: string): { label: HTMLLabelElement; input: HTMLInputElement } {
  const label = create("label", "", text);
  label.htmlFor = id;
  const input = create("input", "");
  input.id = id;
  input.type = type;
  input.autocomplete = "off";
  input.spellcheck = false;
  return { label, input };
}

function setText(node: HTMLElement, text: string): void {
  if (node.textContent !== text) node.textContent = text;
}

export function createBotPanel({ monitor, platform, openWhatsapp }: BotPanelDependencies): HTMLElement {
  const panel = create("section", "bot-panel");
  panel.id = BOT_PANEL_ID;
  panel.tabIndex = -1;
  panel.setAttribute("aria-labelledby", "bot-panel-title");
  const title = create("h5", "subtitle", "Mi bot");
  title.id = "bot-panel-title";

  const linkForm = create("form", "bot-form");
  linkForm.noValidate = true;
  const linkIntro = create("p", "", "Escanea con la cámara el QR que muestra tu bot al arrancar, o pega su enlace.");
  linkIntro.id = "bot-connection-link-intro";
  const connectionLink = field("bot-connection-link", "Enlace del bot", "password");
  connectionLink.input.setAttribute("aria-describedby", linkIntro.id);
  const connectLink = create("button", "button-main", "Conectar");
  connectLink.type = "submit";
  linkForm.append(linkIntro, connectionLink.label, connectionLink.input, connectLink);

  const advanced = create("details", "bot-advanced");
  const advancedTitle = create("summary", "", "Opciones avanzadas");
  const form = create("form", "bot-form");
  form.noValidate = true;
  const url = field("bot-url", "Dirección del bot", "url");
  url.input.inputMode = "url";
  url.input.placeholder = "https://equipo.tu-red.ts.net";
  const token = field("bot-token", "Token (contenido del archivo control-token del bot)", "password");
  const tokenHint = create("p", "secondary small");
  tokenHint.id = "bot-token-hint";
  token.input.setAttribute("aria-describedby", tokenHint.id);
  const connect = create("button", "button-secondary", "Conectar con estos datos");
  connect.type = "submit";
  form.append(url.label, url.input, token.label, token.input, tokenHint, connect);
  const disable = button("button-secondary", "Deshabilitar");
  const disableHint = create("p", "secondary small", "«Deshabilitar» solo desconecta esta app: no desvincula ni cambia el bot.");
  const unlink = button("button-text button-text-danger", "Desvincular…");

  const status = create("p", "bot-state");
  status.setAttribute("role", "status");
  const notice = create("p", "secondary small");
  notice.setAttribute("role", "status");
  const failure = create("p", "message-error");
  failure.setAttribute("role", "alert");
  failure.hidden = true;

  const actions = create("div", "actions-row");
  const activate = button("button-main", "Activar");
  const pause = button("button-secondary", "Pausar");
  const linkCode = button("button-secondary", "Vincular con código");
  const linkQr = button("button-secondary", "Vincular con QR");
  actions.append(activate, pause, linkCode, linkQr);

  const phoneBox = create("div", "bot-form");
  const phone = field("bot-pair-phone", "Número del WhatsApp a vincular", "tel");
  phone.input.inputMode = "tel";
  phone.input.placeholder = "+34 612 345 678";
  const phoneHint = create("p", "secondary small", "Con código de país. La app no lo guarda.");
  phoneHint.id = "bot-pair-phone-hint";
  phone.input.setAttribute("aria-describedby", phoneHint.id);
  phoneBox.append(phone.label, phone.input, phoneHint);

  const confirmation = create("div", "panel-confirm");
  confirmation.setAttribute("role", "group");
  const question = create("p", "", "¿Desvincular el WhatsApp del bot? Dejará de funcionar hasta que lo vincules otra vez.");
  question.id = "bot-unlink-question";
  confirmation.setAttribute("aria-labelledby", question.id);
  const confirmUnlink = button("button-danger", "Desvincular");
  const cancelUnlink = button("button-secondary", "Cancelar");
  const confirmRow = create("div", "actions-row");
  confirmRow.append(confirmUnlink, cancelUnlink);
  confirmation.append(question, confirmRow);
  confirmation.hidden = true;

  const pairing = create("div", "bot-pairing");
  const pairingNote = create("p", "small", "Estos pasos se hacen en el WhatsApp que quedará vinculado al bot.");
  const steps = create("ol", "bot-steps small");
  const codeBox = create("div", "bot-code-box");
  const code = create("output", "bot-code");
  code.setAttribute("aria-label", "Código de vinculación");
  const copy = button("button-secondary", "Copiar");
  copy.setAttribute("aria-label", "Copiar el código de vinculación");
  const whatsapp = button("button-secondary", "Abrir WhatsApp");
  const copied = create("p", "secondary small");
  copied.setAttribute("role", "status");
  codeBox.append(code, copy, whatsapp, copied);
  const qrBox = create("div", "bot-qr");
  const qrNote = create("p", "small");
  pairing.append(pairingNote, steps, codeBox, qrNote, qrBox);

  advanced.append(advancedTitle, form, disable, disableHint, unlink, confirmation);
  panel.append(title, status, notice, linkForm, phoneBox, actions, pairing, advanced, failure);

  let busy = false;
  let drawnQr: string | null = null;
  let shownSteps: BotLinkMethod | null = null;
  let filledUrl: string | null | undefined;
  let visible = false;
  let triedLocal = false;
  let shownEnabled: boolean | null = null;

  function showPairing(state: BotState): void {
    const current = botEnabled(state) && state.status?.link === "linking" ? state.status : null;
    pairing.hidden = current === null;
    if (current === null || current.code !== code.textContent) setText(copied, "");
    setText(code, current?.code ?? "");
    codeBox.hidden = current?.code == null;
    whatsapp.hidden = platform !== "android";
    const qr = current?.qr ?? null;
    const advice = qrNoteText(platform, qr);
    qrNote.hidden = advice === null;
    setText(qrNote, advice ?? "");
    if (qr !== drawnQr) {
      drawnQr = qr;
      let image: SVGSVGElement | null = null;
      try {
        image = qr === null ? null : qrImage(qr);
      } catch {
        showMessage(failure, "No se pudo dibujar el código QR. Prueba a vincular con código.");
      }
      qrBox.replaceChildren(...(image === null ? [] : [image]));
    }
    qrBox.hidden = drawnQr === null;
    // Until the bot sends the code or the QR the method is unknown, unless it was asked from here.
    const method: BotLinkMethod = current?.qr != null ? "qr" : current?.code != null ? "code" : (shownSteps ?? "code");
    if (steps.childElementCount === 0 || method !== shownSteps) {
      shownSteps = method;
      steps.replaceChildren(...linkSteps(method).map((text) => create("li", "", text)));
    }
  }

  function render(state: BotState): void {
    const enabled = botEnabled(state);
    if (enabled !== shownEnabled) {
      if (shownEnabled !== null) advanced.open = false;
      shownEnabled = enabled;
    }
    const saved = state.connection?.url ?? null;
    if (saved !== filledUrl) {
      filledUrl = saved;
      url.input.value = saved ?? "";
    }
    url.input.readOnly = enabled;
    linkForm.hidden = enabled;
    // Nothing saved: only the link is offered.
    advanced.hidden = saved === null;
    status.hidden = !enabled;
    for (const node of [token.label, token.input, tokenHint, connect]) node.hidden = enabled;
    disable.hidden = !enabled;
    disableHint.hidden = !enabled;
    setText(
      tokenHint,
      state.connection?.has_token === true
        ? "Hay un token guardado: déjalo vacío para usarlo. Si cambias la dirección, tienes que pegarlo otra vez."
        : "Se guarda en esta app y no se vuelve a mostrar.",
    );

    const description = describeBot(state);
    setText(status, description.text);
    status.dataset["kind"] = description.kind;
    const noticeText = botNotice(state);
    notice.hidden = !enabled || noticeText === null;
    setText(notice, noticeText ?? "");

    const available = botActions(state);
    activate.hidden = !available.activate;
    pause.hidden = !available.pause;
    linkCode.hidden = !available.link;
    linkQr.hidden = !available.link;
    phoneBox.hidden = !phoneFieldVisible(state);
    unlink.hidden = !available.unlink;
    actions.hidden = !Object.values(available).some(Boolean);
    if (!available.unlink) confirmation.hidden = true;
    for (const node of [connectLink, connect, disable, activate, pause, linkCode, linkQr, unlink, confirmUnlink]) node.disabled = busy;
    showPairing(state);
    if (!triedLocal && shouldConnectLocal(platform, state, visible)) {
      triedLocal = true;
      void attempt(() => monitor.connectLocal());
    }
  }

  async function attempt(work: () => Promise<unknown>): Promise<void> {
    if (busy) return;
    busy = true;
    showMessage(failure, null);
    render(monitor.state());
    try {
      await work();
      showMessage(failure, null);
    } catch (error) {
      showMessage(failure, error instanceof Error ? error.message : errorMessage(asErrorUi(error)));
    } finally {
      busy = false;
      render(monitor.state());
    }
  }

  linkForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (botEnabled(monitor.state())) return;
    const typed = connectionLink.input.value.trim();
    void attempt(async () => {
      await monitor.connectLink(typed);
      connectionLink.input.value = "";
    });
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (botEnabled(monitor.state())) return;
    const typed = token.input.value.trim();
    void attempt(async () => {
      await monitor.connect(url.input.value, typed === "" ? null : typed);
      token.input.value = "";
    });
  });
  new IntersectionObserver((entries) => {
    visible = entries[entries.length - 1]?.isIntersecting ?? visible;
    render(monitor.state());
  }).observe(panel);
  disable.addEventListener("click", () => void attempt(() => monitor.disable()));
  activate.addEventListener("click", () => void attempt(() => monitor.run((api) => api.activate())));
  pause.addEventListener("click", () => void attempt(() => monitor.run((api) => api.pause())));
  for (const [node, method] of [[linkCode, "code"], [linkQr, "qr"]] as const) {
    node.addEventListener("click", () => {
      submitLink(
        method,
        phone.input.value,
        (chosen, number) => {
          shownSteps = chosen;
          steps.replaceChildren(...linkSteps(chosen).map((text) => create("li", "", text)));
          void attempt(() => monitor.run((api) => api.link(chosen, number)));
        },
        () => {
          showMessage(failure, "El número no es válido: escribe de 8 a 15 dígitos, con código de país.");
          phone.input.focus();
        },
      );
    });
  }
  unlink.addEventListener("click", () => {
    confirmation.hidden = false;
    cancelUnlink.focus();
  });
  cancelUnlink.addEventListener("click", () => {
    confirmation.hidden = true;
    unlink.focus();
  });
  confirmUnlink.addEventListener("click", () => {
    confirmation.hidden = true;
    void attempt(() => monitor.run((api) => api.unlink()));
  });
  copy.addEventListener("click", () => {
    navigator.clipboard.writeText(code.textContent ?? "").then(
      () => setText(copied, "Código copiado."),
      () => setText(copied, "No se pudo copiar: selecciona el código y cópialo a mano."),
    );
  });
  whatsapp.addEventListener("click", () => openWhatsapp());

  monitor.subscribe(render);
  render(monitor.state());
  watchVisibility(monitor, "panel", panel);
  return panel;
}
