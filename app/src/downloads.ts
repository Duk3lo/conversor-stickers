import { asErrorUi, type UiError, type QueueEvent } from "./contract.ts";

export function createDownloads(dependencies: {
  download(emit: (event: QueueEvent) => void): Promise<void>;
  onStart(): void;
  onEvent(event: QueueEvent): void;
  onFail(error: UiError): void;
}): { start(): void; cancel(): void } {
  let inGear = false;
  let timeOther = false;

  async function download(): Promise<void> {
    inGear = true;
    dependencies.onStart();
    do {
      timeOther = false;
      try {
        await dependencies.download((event) => {
          if (event.kind === "change") dependencies.onEvent(event);
        });
      } catch (failure) {
        dependencies.onFail(asErrorUi(failure));
      }
    } while (timeOther);
    inGear = false;
    dependencies.onEvent({ kind: "finished" });
  }

  return {
    start() {
      if (inGear) timeOther = true;
      else void download();
    },
    cancel() {
      timeOther = false;
    },
  };
}
