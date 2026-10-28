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
