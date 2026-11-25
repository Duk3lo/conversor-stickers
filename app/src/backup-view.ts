import { element, showMessage } from "./dom.ts";
import type { BackupActions, BackupView } from "./views.ts";

export function createBackupView(actions: BackupActions): { render(view: BackupView): void } {
  const doExport = element<HTMLButtonElement>("backup-export");
  const restore = element<HTMLButtonElement>("backup-restore");
  const confirm = element<HTMLDivElement>("backup-confirm");
  const confirmSi = element<HTMLButtonElement>("backup-confirm-si");
  const confirmNo = element<HTMLButtonElement>("backup-confirm-no");
  const state = element<HTMLParagraphElement>("backup-state");
  const error = element<HTMLParagraphElement>("backup-error");

  doExport.addEventListener("click", actions.doExport);
  restore.addEventListener("click", actions.requestRestore);
  confirmSi.addEventListener("click", actions.restore);
  confirmNo.addEventListener("click", actions.cancelRestore);

  let beforeConfirmed = false;
  return {
    render(view) {
      const busy = view.busy !== null;
      doExport.disabled = busy;
      restore.disabled = busy || view.confirming;
      confirmSi.disabled = busy;
      confirm.hidden = !view.confirming;
      showMessage(state, view.busy ?? view.notice);
      showMessage(error, view.error);
      if (view.confirming && !beforeConfirmed) confirmNo.focus({ preventScroll: true });
      else if (!view.confirming && beforeConfirmed && !busy) restore.focus({ preventScroll: true });
      beforeConfirmed = view.confirming;
    },
  };
}
