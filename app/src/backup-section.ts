import type { CatalogApi } from "./catalog-api.ts";
import { type LibraryUi, asErrorUi } from "./contract.ts";
import { errorMessage } from "./messages.ts";
import { createBackupView } from "./backup-view.ts";
import type { BackupView } from "./views.ts";

export interface BackupDependencies {
  readonly api: CatalogApi;
  readonly setLibrary: (library: LibraryUi) => void;
}

export function createBackupSection(deps: BackupDependencies): void {
  let view: BackupView = { busy: null, confirming: false, notice: null, error: null };

  function set(changes: Partial<BackupView>): void {
    view = { ...view, ...changes };
    screen.render(view);
  }

  async function run(text: string, task: () => Promise<string | null>): Promise<void> {
    set({ busy: text, confirming: false, notice: null, error: null });
    try {
      set({ busy: null, notice: await task() });
    } catch (failure) {
      set({ busy: null, error: errorMessage(asErrorUi(failure)) });
    }
  }

  const screen = createBackupView({
    doExport: () =>
      void run("Guardando la copia…", async () => {
        const path = await deps.api.exportBackup();
        return path === null ? null : `Copia guardada en ${path}.`;
      }),
    requestRestore: () => set({ confirming: true, notice: null, error: null }),
    cancelRestore: () => set({ confirming: false }),
    restore: () =>
      void run("Restaurando la copia…", async () => {
        const library = await deps.api.restoreBackup();
        if (library === null) return null;
        deps.setLibrary(library);
        return "Copia restaurada. Tu biblioteca anterior se guardó aparte.";
      }),
  });
  screen.render(view);
}
