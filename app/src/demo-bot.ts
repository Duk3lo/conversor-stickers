// A pretend bot for the demo: it connects, links and changes state without any network.
import type { BotApi, BotConnectionUi, BotStatusUi } from "./contract-bot.ts";

export function createBotDemoApi(): BotApi {
  let connection: BotConnectionUi = { url: null, has_token: false, enabled: false };
  let status: BotStatusUi = { link: "unlinked", connection: "disconnected", active: false, code: null, qr: null, notice: null };
  const change = async (changes: Partial<BotStatusUi>): Promise<BotStatusUi> => (status = { ...status, ...changes });
  return {
    connection: async () => connection,
    connect: async (url) => (connection = { url, has_token: true, enabled: true }),
    disable: async () => (connection = { ...connection, enabled: false }),
    status: async () => status,
    activate: () => change({ active: true }),
    pause: () => change({ active: false }),
    link: (method) =>
      change({
        link: "linking",
        connection: "connecting",
        code: method === "code" ? "ABCD-EFGH" : null,
        qr: method === "qr" ? "2@demostracion,no-vincula-nada" : null,
      }),
    unlink: () => change({ link: "unlinked", connection: "disconnected", active: false }),
    openChat: async (number) => console.info("demo: abrir chat del bot", number.length),
  };
}
