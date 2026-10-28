import { t } from "../../../../../support/ui/i18n";
import { GitCommitHorizontal, X } from "lucide-react";
import type { HistoryEntry } from "../types";

export function CommitPicker({
  entries,
  busy,
  onClose,
  onSelect,
}: {
  entries: HistoryEntry[];
  busy: boolean;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="git-sheet" role="dialog" aria-label={t("选择 commit")}>
      <div className="git-sheet-card git-history-card">
        <header>
          <strong>{t("选择 commit")}</strong>
          <button type="button" aria-label={t("关闭")} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="git-history-list">
          {!entries.length && (
            <p className="git-panel-empty">{t("暂无提交记录")}</p>
          )}
          {entries.map((item) => (
            <button
              key={item.id}
              className={`git-history-item${item.current ? " active" : ""}`}
              disabled={busy}
              onClick={() => {
                onClose();
                onSelect(item.id);
              }}
            >
              <span className="git-history-marker">
                <GitCommitHorizontal size={17} />
              </span>
              <span>
                <strong>{item.subject}</strong>
                <small>
                  {item.author} ·{" "}
                  {new Date(item.timestamp * 1000).toLocaleString("zh-CN")}
                </small>
                <code>
                  {item.shortId}
                  {item.current && (
                    <span className="git-head-badge">{t("当前")}</span>
                  )}
                </code>
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
