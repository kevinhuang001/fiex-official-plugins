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
    <div className="git-sheet git-remote-sheet" role="dialog" aria-label="远程仓库">
      <div className="git-sheet-card git-remote-card">
        <header>
          <strong>
            {editing
              ? editing.mode === "new"
                ? "添加远程仓库"
                : "编辑远程仓库"
              : "远程仓库"}
          </strong>
          <button type="button" aria-label="关闭" onClick={onClose}>
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
              名称
              <input
                aria-label="远程名称"
                value={editing.name}
                required
                placeholder="origin"
                onChange={(event) =>
                  onEditingChange({ ...editing, name: event.target.value })
                }
              />
            </label>
            <label>
              仓库地址
              <input
                aria-label="仓库地址"
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
                返回
              </button>
              <button
                className="primary"
                type="submit"
                disabled={
                  busy || !editing.name.trim() || !editing.url.trim()
                }
              >
                保存
              </button>
            </div>
          </form>
        ) : (
          <>
            <section className="git-remote-section">
              <div className="git-remote-section-title">
                <span>已配置</span>
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
                  aria-label="远程仓库"
                >
                  {remote.remotes.map((item) => (
                    <li
                      key={item.name}
                      className={`git-remote-row ${item.name === remoteName ? "active" : ""}`}
                    >
                      {pendingRemove === item.name ? (
                        <div className="git-remote-confirm">
                          <span>移除 {item.name}？</span>
                          <button onClick={() => onPendingRemoveChange(null)}>
                            取消
                          </button>
                          <button
                            className="git-danger"
                            disabled={busy}
                            onClick={() => onRemoveRemote(item.name)}
                          >
                            移除
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
                            <span className="git-remote-name">
                              {item.name}
                            </span>
                            <span className="git-remote-url" title={item.url}>
                              {item.url}
                            </span>
                            {item.name === remoteName && (
                              <Check size={13} className="git-remote-check" />
                            )}
                          </button>
                          <button
                            className="icon-button"
                            title={`编辑 ${item.name}`}
                            aria-label={`编辑 ${item.name}`}
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
                            title={`移除 ${item.name}`}
                            aria-label={`移除 ${item.name}`}
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
                <p className="git-remote-empty">无远程仓库</p>
              )}
              <button
                className="git-remote-add"
                disabled={busy}
                onClick={() =>
                  onEditingChange({ mode: "new", name: "", url: "" })
                }
              >
                <Plus size={13} /> 添加远程仓库
              </button>
            </section>
            <section className="git-remote-sync">
              <label className="git-remote-branch">
                分支
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
                <CloudDownload size={14} /> 拉取
              </button>
              <button
                disabled={busy || !remoteName}
                onClick={() => onSync(true)}
              >
                <CloudUpload size={14} /> 推送
              </button>
            </section>
            {remote?.upstream && (
              <small className="git-remote-upstream">
                {remote.upstream}
                {remote.ahead ? ` · 领先 ${remote.ahead}` : ""}
                {remote.behind ? ` · 落后 ${remote.behind}` : ""}
              </small>
            )}
          </>
        )}
      </div>
    </div>
  );
}
