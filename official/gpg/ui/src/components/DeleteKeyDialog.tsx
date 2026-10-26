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
        aria-label="删除密钥"
      >
        <span className="gpg-confirm-icon" aria-hidden>
          <AlertTriangle size={18} />
        </span>
        <h3>删除密钥？</h3>
        <p>
          {selected.name} 的{selected.hasSecret ? "公钥和私钥" : "公钥"}将从本机
          GnuPG 钥匙串中删除，此操作无法撤销。
        </p>
        <footer>
          <button type="button" onClick={onCancel} disabled={busy}>
            取消
          </button>
          <button
            type="button"
            className="danger"
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? "正在删除…" : "删除"}
          </button>
        </footer>
      </div>
    </div>
  );
}
