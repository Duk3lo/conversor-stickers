// A pretend bot for the demo: it connects, links and changes state without any network.
import type { BotApi, BotConnectionUi, BotStatusUi } from "./contract-bot.ts";

export function createBotDemoApi(): BotApi {
  let connection: BotConnectionUi = { url: null, has_token: false, enabled: false };
  let status: BotStatusUi = { link: "unlinked", connection: "disconnected", active: false, code: null, qr: null, notice: null };
  const change = async (changes: Partial<BotStatusUi>): Promise<BotStatusUi> => (status = { ...status, ...changes });
  return {
    connection: async () => connection,
    connect: async (url) => (connection = { url, has_token: true, enabled: true }),
    connectLink: async (link) => {
      const url = /u=([^&]*)/.exec(link)?.[1];
      return (connection = { url: url === undefined ? "http://127.0.0.1:8787" : decodeURIComponent(url), has_token: true, enabled: true });
    },
    receiveLink: async () => null,
    acceptLink: async () => connection,
    rejectLink: async () => undefined,
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
