import { t } from "../../../../../support/ui/i18n";
import {
  Check,
  Cloud,
  CloudDownload,
  CloudUpload,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import type { Refs, RemoteEdit, RemoteStatus } from "../types";
import { BranchSelect } from "./BranchSelect";

export function RemoteDialog({
  remote,
  refs,
  remoteName,
  busy,
  editing,
  pendingRemove,
  syncBranch,
  onEditingChange,
  onPendingRemoveChange,
  onSyncBranchChange,
  onSelectRemote,
  onSaveRemote,
  onRemoveRemote,
  onSync,
  onClose,
}: {
  remote: RemoteStatus | null;
  refs: Refs | null;
  remoteName: string;
  busy: boolean;
  editing: RemoteEdit | null;
  pendingRemove: string | null;
  syncBranch: string;
  onEditingChange: (editing: RemoteEdit | null) => void;
  onPendingRemoveChange: (name: string | null) => void;
  onSyncBranchChange: (branch: string) => void;
  onSelectRemote: (name: string) => void;
  onSaveRemote: (name: string, url: string) => void;
  onRemoveRemote: (name: string) => void;
  onSync: (push: boolean) => void;
  onClose: () => void;
}) {
  return (
    <div
      className="git-sheet git-remote-sheet"
      role="dialog"
      aria-label={t("远程仓库")}
    >
      <div className="git-sheet-card git-remote-card">
        <header>
          <strong>
            {editing
              ? editing.mode === "new"
                ? t("添加远程仓库")
                : t("编辑远程仓库")
              : t("远程仓库")}
          </strong>
          <button type="button" aria-label={t("关闭")} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        {editing ? (
          <form
            className="git-remote-form"
            onSubmit={(event) => {
              event.preventDefault();
              onSaveRemote(editing.name, editing.url);
            }}
          >
            <label>
              {t("名称")}{" "}
              <input
                aria-label={t("远程名称")}
                value={editing.name}
                required
                placeholder="origin"
                onChange={(event) =>
                  onEditingChange({ ...editing, name: event.target.value })
                }
              />
            </label>
            <label>
              {t("仓库地址")}{" "}
              <input
                aria-label={t("仓库地址")}
                value={editing.url}
                required
                placeholder="https://… / git@…"
                onChange={(event) =>
                  onEditingChange({ ...editing, url: event.target.value })
                }
              />
            </label>
            <div className="git-remote-form-actions">
              <button
                type="button"
                disabled={busy}
                onClick={() => onEditingChange(null)}
              >
                {t("返回")}{" "}
              </button>
              <button
                className="primary"
                type="submit"
                disabled={busy || !editing.name.trim() || !editing.url.trim()}
              >
                {t("保存")}{" "}
              </button>
            </div>
          </form>
        ) : (
          <>
            <section className="git-remote-section">
              <div className="git-remote-section-title">
                <span>{t("已配置")}</span>
                <span className="git-remote-badge">
                  {remote?.remotes.length || 0}
                </span>
                <span className="git-remote-count">
                  ↓ {remote?.behind || 0} · ↑ {remote?.ahead || 0}
                </span>
              </div>
              {remote?.remotes.length ? (
                <ul
                  className="git-remote-list"
                  role="listbox"
                  aria-label={t("远程仓库")}
                >
                  {remote.remotes.map((item) => (
                    <li
                      key={item.name}
                      className={`git-remote-row ${item.name === remoteName ? "active" : ""}`}
                    >
                      {pendingRemove === item.name ? (
                        <div className="git-remote-confirm">
                          <span>{t("移除 {0}？", item.name)}</span>
                          <button onClick={() => onPendingRemoveChange(null)}>
                            {t("取消")}{" "}
                          </button>
                          <button
                            className="git-danger"
                            disabled={busy}
                            onClick={() => onRemoveRemote(item.name)}
                          >
                            {t("移除")}{" "}
                          </button>
                        </div>
                      ) : (
                        <>
                          <button
                            role="option"
                            aria-selected={item.name === remoteName}
                            className="git-remote-select"
                            onClick={() => {
                              onSelectRemote(item.name);
                            }}
                          >
                            <Cloud size={14} />
                            <span className="git-remote-name">{item.name}</span>
                            <span className="git-remote-url" title={item.url}>
                              {item.url}
                            </span>
                            {item.name === remoteName && (
                              <Check size={13} className="git-remote-check" />
                            )}
                          </button>
                          <button
                            className="icon-button"
                            title={t("编辑 {0}", item.name)}
                            aria-label={t("编辑 {0}", item.name)}
                            disabled={busy}
                            onClick={() =>
                              onEditingChange({
                                mode: "edit",
                                name: item.name,
                                url: item.url,
                              })
                            }
                          >
                            <Pencil size={13} />
                          </button>
                          <button
                            className="icon-button"
                            title={t("移除 {0}", item.name)}
                            aria-label={t("移除 {0}", item.name)}
                            disabled={busy}
                            onClick={() => onPendingRemoveChange(item.name)}
                          >
                            <Trash2 size={13} />
                          </button>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="git-remote-empty">{t("无远程仓库")}</p>
              )}
              <button
                className="git-remote-add"
                disabled={busy}
                onClick={() =>
                  onEditingChange({ mode: "new", name: "", url: "" })
                }
              >
                <Plus size={13} /> {t("添加远程仓库")}{" "}
              </button>
            </section>
            <section className="git-remote-sync">
              <label className="git-remote-branch">
                {t("分支")}{" "}
                <BranchSelect
                  value={syncBranch || refs?.current || ""}
                  options={
                    refs?.branches.length
                      ? refs.branches
                      : refs?.current
                        ? [refs.current]
                        : []
                  }
                  onChange={onSyncBranchChange}
                />
              </label>
              <button
                disabled={busy || !remoteName}
                onClick={() => onSync(false)}
              >
                <CloudDownload size={14} /> {t("拉取")}{" "}
              </button>
              <button
                disabled={busy || !remoteName}
                onClick={() => onSync(true)}
              >
                <CloudUpload size={14} /> {t("推送")}{" "}
              </button>
            </section>
            {remote?.upstream && (
              <small className="git-remote-upstream">
                {remote.upstream}
                {remote.ahead ? t(" · 领先 {0}", remote.ahead) : ""}
                {remote.behind ? t(" · 落后 {0}", remote.behind) : ""}
              </small>
            )}
          </>
        )}
      </div>
    </div>
  );
}
