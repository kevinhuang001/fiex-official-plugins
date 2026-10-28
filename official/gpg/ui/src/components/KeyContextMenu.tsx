import { t } from "../../../../../support/ui/i18n";
import { useEffect, useRef } from "react";
import { Download, Trash2 } from "lucide-react";
import type { Key } from "../types";

/** Right-click menu for a key row. The panel is fixed inside the iframe, so
 *  the position is clamped to its viewport. */
export function KeyContextMenu({
  item: selected,
  x,
  y,
  onExport,
  onExportSecret,
  onDelete,
  onClose,
}: {
  item: Key;
  x: number;
  y: number;
  onExport: (key: Key) => void;
  onExportSecret: (key: Key) => void;
  onDelete: (key: Key) => void;
  onClose: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (event: Event) => {
      if (!menu.current?.contains(event.target as Node)) onClose();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", escape);
    };
  }, [onClose]);
  const style = {
    left: Math.min(x, window.innerWidth - 184),
    top: Math.min(y, window.innerHeight - (selected.hasSecret ? 128 : 96)),
  };
  return (
    <div
      ref={menu}
      className="gpg-key-menu"
      style={style}
      role="menu"
      aria-label={t("密钥操作")}
    >
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          onExport(selected);
          onClose();
        }}
      >
        <Download size={13} />
        {t("导出公钥")}{" "}
      </button>
      {selected.hasSecret && (
        <button
          type="button"
          role="menuitem"
          onClick={() => {
            onExportSecret(selected);
            onClose();
          }}
        >
          <Download size={13} />
          {t("导出私钥")}{" "}
        </button>
      )}
      <button
        type="button"
        role="menuitem"
        className="danger"
        onClick={() => {
          onDelete(selected);
          onClose();
        }}
      >
        <Trash2 size={13} />
        {t("删除密钥")}{" "}
      </button>
    </div>
  );
}
