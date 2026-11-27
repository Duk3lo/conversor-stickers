// Android: the bot's QR, scanned with the camera, opens the app with a connection link. Nothing is
// saved until the person confirms the address. The token stays in Rust and is never shown.
import type { BotApi } from "./contract-bot.ts";
import type { BotMonitor } from "./bot-monitor.ts";
import { offerReceivedLink, oneAtATime } from "./bot-status.ts";
import { create } from "./dom.ts";

function ask(question: string): Promise<boolean> {
  return new Promise((resolve) => {
    const dialog = create("dialog", "bot-dialog");
    dialog.setAttribute("aria-labelledby", "bot-link-question");
    const text = create("p", "", question);
    text.id = "bot-link-question";
    const yes = create("button", "button-main", "Conectar");
    const no = create("button", "button-secondary", "Cancelar");
    yes.type = no.type = "button";
    const row = create("div", "actions-row");
    row.append(yes, no);
    dialog.append(text, row);
    let accepted = false;
    yes.addEventListener("click", () => {
      accepted = true;
      dialog.close();
    });
    no.addEventListener("click", () => dialog.close());
    // Escape or any other way of closing is a refusal.
    dialog.addEventListener("close", () => {
      dialog.remove();
      resolve(accepted);
    });
    document.body.append(dialog);
    dialog.showModal();
    no.focus();
  });
}

export function watchReceivedLinks(api: BotApi, monitor: BotMonitor, fail: (error: unknown) => void): void {
  // A link that arrives while the dialog is open is offered again when it closes.
  const offer = oneAtATime(() =>
    offerReceivedLink({
      receive: () => api.receiveLink(),
      ask,
      accept: () => monitor.acceptLink(),
      reject: () => api.rejectLink(),
      fail,
    }),
  );
  window.addEventListener("android-bot-link", () => void offer());
  void offer();
}
