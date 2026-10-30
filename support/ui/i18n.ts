import { createI18n, FiexUiClient } from "../../sdk/ui/src/index";
import en from "../i18n/en.json";
import traditional from "../i18n/zh-TW.json";

/** One runtime per panel, initialized only from Core's bridge handshake. */
export const client = new FiexUiClient();
export const i18n = createI18n({ en, "zh-TW": traditional });
export const { t, getLocale } = i18n;
export const ready = client.i18n.bind(i18n);
i18n.subscribe(() => {
  document.documentElement.lang = getLocale();
});
void ready.then(() => {
  document.documentElement.lang = getLocale();
});

// All native official backends register SDK diagnostics endpoints. There is no
// direct Tauri/Host call, plugin identity parameter or second local log store.
void ready.then(() => client.logs.write("debug", "Plugin UI ready")).catch(() => {});
window.addEventListener("error", (event) => {
  if (!event.error) return; // Resource download events have no render exception.
  void client.errors.report(t("插件界面发生错误"), String(event.error));
});
window.addEventListener("unhandledrejection", (event) => {
  void client.errors.report(t("插件界面操作未完成"), String(event.reason));
});
