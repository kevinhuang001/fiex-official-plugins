import { t } from "../../../../../support/ui/i18n";
import { AlertTriangle } from "lucide-react";
import type { Key } from "../types";

export function DeleteKeyDialog({
  item: selected,
  busy,
  onCancel,
  onConfirm,
}: {
  item: Key;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="gpg-confirm-backdrop">
      <div
        className="gpg-confirm"
        role="dialog"
        aria-modal="true"
        aria-label={t("删除密钥")}
      >
        <span className="gpg-confirm-icon" aria-hidden>
          <AlertTriangle size={18} />
        </span>
        <h3>{t("删除密钥？")}</h3>
        <p>
          {t(
            "将从本机 GnuPG 钥匙串中删除 {0} 的{1}，此操作无法撤销。",
            selected.name,
            selected.hasSecret ? t("公钥和私钥") : t("公钥"),
          )}
        </p>
        <footer>
          <button type="button" onClick={onCancel} disabled={busy}>
            {t("取消")}{" "}
          </button>
          <button
            type="button"
            className="danger"
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? t("正在删除…") : t("删除")}
          </button>
        </footer>
      </div>
    </div>
  );
}
