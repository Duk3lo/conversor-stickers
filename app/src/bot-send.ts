// «Enviar al bot»: save the clip, ask the bot how it is, activate it if it is paused and hand the clip
// over. It is activated last so that a cancelled save never leaves it active, and asked after saving,
// which can take long. If any step fails or the person cancels, nothing after it happens.
import type { BotStatusUi } from "./contract-bot.ts";
import { asErrorUi } from "./contract.ts";
import { errorMessage } from "./messages.ts";

export interface BotSendSteps {
  status(): Promise<BotStatusUi>;
  activate(): Promise<BotStatusUi>;
  /** The saved name, or `null` if the person cancelled. */
  save(): Promise<string | null>;
  /** Hands the saved clip to WhatsApp and says what is left to do. */
  deliver(name: string): Promise<string>;
  /** The editor was closed or changed meanwhile. */
  abandoned(): boolean;
  progress(text: string): void;
}

export interface BotSendOutcome {
  readonly sent: boolean;
  readonly message: string;
}

/** «Enviar al bot» in the editor. */
export type BotSendState =
  | { readonly kind: "idle" }
  | { readonly kind: "running"; readonly text: string }
  | {
      readonly kind: "done";
      readonly sent: boolean;
      readonly message: string;
      /** On desktop, once the clip is saved: what has to be pasted next to it, if anything. */
      readonly caption: string | null;
    };

const STILL_ACTIVE = " El bot quedó activo; puedes pausarlo en «Mi bot».";
// An activation whose answer never arrived may have reached the bot.
const MAYBE_ACTIVE = " El bot puede haber quedado activo; compruébalo en «Mi bot».";

function messageOf(failure: unknown): string {
  return failure instanceof Error ? failure.message : errorMessage(asErrorUi(failure));
}

export async function runBotSend(steps: BotSendSteps): Promise<BotSendOutcome> {
  let leftActive = "";
  const notSent = (why: string): BotSendOutcome => ({ sent: false, message: `No se ha enviado nada. ${why}${leftActive}` });
  let doing = "No se pudo guardar el clip";
  let activating = false;
  try {
    steps.progress("Guardando el clip…");
    const name = await steps.save();
    if (name === null) return notSent("Se canceló el guardado del clip.");
    if (steps.abandoned()) return notSent("Se cerró el editor.");
    doing = "El clip se guardó, pero no se pudo consultar el bot";
    steps.progress("Consultando el bot…");
    const status = await steps.status();
    if (steps.abandoned()) return notSent("Se cerró el editor.");
    if (status.link !== "linked") return notSent("Primero vincula el WhatsApp del bot en «Mi bot».");
    if (!status.active) {
      doing = "No se pudo activar el bot";
      steps.progress("Activando el bot…");
      activating = true;
      if (!(await steps.activate()).active) return notSent("El bot no se ha activado.");
      activating = false;
      leftActive = STILL_ACTIVE;
      if (steps.abandoned()) return notSent("Se cerró el editor.");
    }
    doing = "El clip se guardó, pero no se pudo abrir WhatsApp";
    steps.progress("Abriendo WhatsApp…");
    return { sent: true, message: await steps.deliver(name) };
  } catch (failure) {
    if (activating && asErrorUi(failure).kind === "bot_unreachable") leftActive = MAYBE_ACTIVE;
    return notSent(`${doing}: ${messageOf(failure)}.`.replace(/\.\.$/, "."));
  }
}

export const ANDROID_DELIVERED = "Se abrió WhatsApp con el clip en el chat del bot. Revísalo y pulsa enviar.";

/** On desktop the person attaches the file by hand, so the message says exactly what to do. */
export function desktopDelivered(name: string, caption: string | null, copied: boolean): string {
  const attach = `Se guardó «${name}» y se abrió el chat del bot. Adjunta ese video`;
  if (caption === null) return `${attach} y envíalo.`;
  const paste = copied ? "pega como pie el texto ya copiado" : "escribe como pie (o usa «Copiar pie»)";
  return `${attach}, ${paste}: ${caption}`;
}
