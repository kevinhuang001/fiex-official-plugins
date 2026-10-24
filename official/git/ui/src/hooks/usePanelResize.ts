import { useEffect } from "react";
import type { FiexUiClient } from "../../../../../sdk/ui/src/index";
import type { Status } from "../types";

export interface PanelResizeState {
  status: Status | null;
  error: string;
  busy: boolean;
  commitOpen: boolean;
  pickerOpen: boolean;
  remoteOpen: boolean;
}

/**
 * The host caps the panel; the plugin asks for the height its content needs
 * so short repositories do not leave a large empty sheet.
 */
export function usePanelResize(client: FiexUiClient, state: PanelResizeState) {
  const { status, error, busy, commitOpen, pickerOpen, remoteOpen } = state;
  useEffect(() => {
    const app = document.querySelector<HTMLElement>(".app");
    if (!app) return;
    const minimum = commitOpen || pickerOpen || remoteOpen ? 430 : 240;
    const height = Math.max(minimum, Math.ceil(app.scrollHeight));
    void client.invoke("panel.resize", { width: 480, height }).catch(() => {});
  }, [client, status, error, busy, commitOpen, pickerOpen, remoteOpen]);
}
