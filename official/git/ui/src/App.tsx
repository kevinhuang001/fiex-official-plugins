import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Check,
  CheckCircle2,
  ChevronDown,
  Cloud,
  CloudDownload,
  CloudUpload,
  FileCode2,
  FolderGit2,
  GitBranch,
  GitCommitHorizontal,
  LoaderCircle,
  Minus,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { FiexUiClient } from "../../../../sdk/ui/src/index";

interface StatusEntry {
  path: string;
  code: string;
}
interface Status {
  root: string;
  branch: string;
  entries: Record<string, string>;
}
interface Refs {
  head: string;
  current: string;
  branches: string[];
  tags: string[];
}
interface Commit {
  id: string;
  shortId: string;
  subject: string;
  author: string;
  timestamp: number;
  refs: string;
}
interface Identity {
  name: string;
  email: string;
}
interface RemoteStatus {
  remotes: { name: string; url: string }[];
  branch: string;
  upstream: string;
  ahead: number;
  behind: number;
}
interface Context {
  directoryPath?: string;
  selectionPaths?: string[];
}

const client = new FiexUiClient();
const name = (path: string) => path.split(/[/\\]/).pop() || path;
const changed = (code: string) => {
  if (code === "??")
    return { letter: "U", label: "未跟踪", staged: false, unstaged: true };
  const index = code[0] ?? " ";
  const work = code[1] ?? " ";
  const staged = index !== " " && index !== "?";
  const unstaged = work !== " " && work !== "?";
  const names: Record<string, string> = {
    M: "修改",
    A: "新增",
    D: "删除",
    R: "重命名",
    C: "复制",
    T: "类型变更",
    U: "冲突",
  };
  const letter = unstaged ? work : index;
  if (letter === " ") return null;
  const label = names[letter] ?? letter;
  return {
    letter,
    label:
      staged && unstaged
        ? `已暂存并${label}`
        : staged
          ? `已暂存${label}`
          : `未暂存${label}`,
    staged,
    unstaged,
  };
};

function BranchSelect({
  value,
  options,
  onChange,
}: {
  value: string;
  options: string[];
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [up, setUp] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  return (
    <div className="branch-select" ref={root}>
      <button
        type="button"
        className="branch-select-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="同步分支"
        onClick={() => {
          const rect = root.current?.getBoundingClientRect();
          setUp(
            Boolean(
              rect &&
              rect.bottom + Math.min(180, options.length * 32 + 8) >
                window.innerHeight - 8,
            ),
          );
          setOpen(!open);
        }}
      >
        {value || "选择分支"}
        <ChevronDown size={13} className={open ? "open" : ""} />
      </button>
      {open && (
        <div
          className={`branch-select-menu${up ? " open-up" : ""}`}
          role="listbox"
          aria-label="同步分支"
        >
          {options.map((item) => (
            <button
              type="button"
              role="option"
              aria-selected={item === value}
              className={`branch-select-option ${item === value ? "active" : ""}`}
              key={item}
              onClick={() => {
                onChange(item);
                setOpen(false);
              }}
            >
              {item}
              {item === value && <Check size={12} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function App() {
  const [context, setContext] = useState<Context>({});
  const [status, setStatus] = useState<Status | null>(null);
  const [refs, setRefs] = useState<Refs | null>(null);
  const [history, setHistory] = useState<Commit[]>([]);
  const [identity, setIdentity] = useState<Identity>({ name: "", email: "" });
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [commitOpen, setCommitOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [remoteOpen, setRemoteOpen] = useState(false);
  const [remote, setRemote] = useState<RemoteStatus | null>(null);
  const [remoteName, setRemoteName] = useState("origin");
  const [remoteEditing, setRemoteEditing] = useState<{
    mode: "new" | "edit";
    name: string;
    url: string;
  } | null>(null);
  const [remotePendingRemove, setRemotePendingRemove] = useState<string | null>(
    null,
  );
  const [syncBranch, setSyncBranch] = useState("");

  const git = useCallback(
    <T,>(method: string, params: Record<string, unknown> = {}) =>
      client.invoke<T>("plugin.invoke", { method, params }),
    [],
  );
  const root = context.directoryPath || "";

  const load = useCallback(async () => {
    if (!root) return;
    setBusy(true);
    setError("");
    try {
      const next = await git<Status | null>("git.status", { root });
      setStatus(next);
      if (next) {
        const [nextRefs, nextHistory, nextIdentity, nextRemote] =
          await Promise.all([
            git<Refs>("git.refs", { root }),
            git<Commit[]>("git.history", { root, skip: 0 }),
            git<Identity>("git.identity", { root }),
            git<RemoteStatus>("git.remoteStatus", { root }),
          ]);
        setRefs(nextRefs);
        setHistory(nextHistory);
        setIdentity(nextIdentity);
        setRemote(nextRemote);
        if (nextRemote.remotes.length) {
          setRemoteName((old) =>
            nextRemote.remotes.some((item) => item.name === old)
              ? old
              : nextRemote.remotes[0].name,
          );
        }
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
      setLoaded(true);
    }
  }, [git, root]);

  useEffect(() => {
    void client
      .invoke<Context>("context.get")
      .then((next) => setContext(next))
      .catch((cause) => setError(String(cause)));
  }, []);
  useEffect(() => {
    if (root) void load();
  }, [root, load]);
  // The host caps the panel; the plugin asks for the height its content needs
  // so short repositories do not leave a large empty sheet.
  useEffect(() => {
    if (!remoteOpen) return;
    setRemoteEditing(null);
    setRemotePendingRemove(null);
    setSyncBranch(refs?.current || "");
  }, [remoteOpen, refs?.current]);
  useEffect(() => {
    const app = document.querySelector(".app") as HTMLElement | null;
    if (!app) return;
    const minimum = commitOpen || pickerOpen || remoteOpen ? 430 : 240;
    const height = Math.max(minimum, Math.ceil(app.scrollHeight));
    void client.invoke("panel.resize", { width: 480, height }).catch(() => {});
  }, [status, error, busy, commitOpen, pickerOpen, remoteOpen]);

  const changes = useMemo(
    () =>
      status
        ? Object.entries(status.entries)
            .sort(([a], [b]) => a.localeCompare(b))
            .flatMap(([path, code]) => {
              const state = changed(code);
              return state ? [{ path, code, ...state }] : [];
            })
        : [],
    [status],
  );
  const staged = changes.filter((item) => item.staged);
  const unstaged = changes.filter((item) => item.unstaged);
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError("");
    try {
      await action();
      await load();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };
  const runTask = async (
    title: string,
    detail: string,
    action: (taskId: string) => Promise<string | void>,
  ) => {
    setBusy(true);
    setError("");
    const task = await client.tasks.create(title, { detail });
    try {
      const result = await action(task.taskId);
      await client.tasks.finish(task.taskId, {
        detail: typeof result === "string" ? result : undefined,
      });
      await load();
    } catch (cause) {
      await client.tasks.finish(task.taskId, { error: String(cause) });
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };
  const toggle = (items: StatusEntry[], staged: boolean) => {
    const paths = items.map((item) => item.path);
    if (!paths.length) return;
    void run(() =>
      git(staged ? "git.unstage" : "git.stage", { root, paths, all: false }),
    );
  };
  const commit = () => {
    if (!message.trim()) return;
    void runTask("提交更改", message.split("\n")[0], async (taskId) => {
      await client.tasks.update(taskId, { step: "写入提交" });
      const hash = await git<string>("git.commit", {
        root,
        message,
        name: identity.name,
        email: identity.email,
      });
      setMessage("");
      setCommitOpen(false);
      return `已提交 ${hash}`;
    });
  };
  const init = () =>
    void runTask("初始化 Git 仓库", root, async (taskId) => {
      await client.tasks.update(taskId, { step: "创建 .git" });
      await git("git.init", { root });
      return "仓库已创建";
    });
  const saveRemote = (nextName: string, nextUrl: string) => {
    if (!nextName.trim() || !nextUrl.trim()) return;
    void run(async () => {
      await git("git.remoteSave", {
        root,
        name: nextName.trim(),
        url: nextUrl.trim(),
      });
      setRemoteName(nextName.trim());
      setRemoteEditing(null);
    });
  };
  const removeRemote = (name: string) =>
    void run(async () => {
      await git("git.remoteRemove", { root, name });
      setRemotePendingRemove(null);
      if (remoteName === name) setRemoteName("origin");
    });
  const sync = (push: boolean) =>
    void runTask(
      push ? "推送分支" : "拉取分支",
      `${remoteName}/${syncBranch || remote?.branch || "当前分支"}`,
      async (taskId) => {
        if (!remoteName.trim()) throw new Error("请先配置远程仓库");
        await client.tasks.update(taskId, {
          step: "连接远程仓库",
          progress: 0.25,
        });
        await git("git.remoteSync", {
          root,
          remote: remoteName,
          push,
          branch: syncBranch || undefined,
        });
        await client.tasks.update(taskId, {
          step: "更新本地状态",
          progress: 0.9,
        });
        return push ? "推送完成" : "拉取完成";
      },
    );
  const openCommit = (id: string) => {
    void git<string>("git.browse", { root, reference: id })
      .then((path) => client.invoke("navigation.open", { path }))
      .then(() => client.invoke("panel.close"))
      .catch((cause) => setError(String(cause)));
  };

  if (!loaded) {
    return (
      <div className="app">
        <div className="git-clean">
          <LoaderCircle size={18} className="spin" />
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {error && (
        <div className="git-panel-error" role="alert">
          {error}
        </div>
      )}
      {busy && (
        <div className="git-progress" role="status">
          <LoaderCircle size={13} className="spin" /> 处理中…
        </div>
      )}
      {!root && (
        <div className="git-welcome">
          <span className="git-welcome-icon">
            <FolderGit2 size={30} />
          </span>
          <p>当前没有可用的文件夹。</p>
        </div>
      )}
      {root && !status && (
        <div className="git-welcome">
          <span className="git-welcome-icon">
            <FolderGit2 size={30} />
          </span>
          <h2>此文件夹还不是 Git 仓库</h2>
          <button className="primary" disabled={busy} onClick={init}>
            初始化仓库
          </button>
        </div>
      )}

      {status && (
        <>
          <div className="git-repository">
            <span className="git-repo-icon">
              <FolderGit2 size={20} />
            </span>
            <span className="git-repo-name">
              <strong>{name(root)}</strong>
            </span>
            <button
              className="git-branch-trigger"
              aria-label="选择 commit"
              disabled={busy}
              onClick={() => setPickerOpen(true)}
            >
              <GitBranch size={13} />
              <span>
                {refs?.current || status.branch} · {refs?.head || "尚无 commit"}
              </span>
              <ChevronDown size={12} />
            </button>
            <button
              className="icon-button"
              title="远程仓库"
              aria-label="远程仓库"
              onClick={() => setRemoteOpen(true)}
            >
              <Cloud size={15} />
            </button>
            <button
              className="icon-button"
              title="刷新"
              aria-label="刷新源代码管理"
              disabled={busy}
              onClick={() => void load()}
            >
              <RefreshCw size={14} className={busy ? "spin" : ""} />
            </button>
          </div>

          <div className="git-commit-action">
            <button
              className="primary"
              disabled={busy || !staged.length}
              onClick={() => setCommitOpen(true)}
            >
              <GitCommitHorizontal size={15} />
              {staged.length ? `提交修改 · ${staged.length}` : "提交修改"}
            </button>
          </div>

          <div className="git-panel-scroll">
            {!changes.length && (
              <div className="git-clean">
                <CheckCircle2 size={28} />
                <strong>工作区干净</strong>
              </div>
            )}
            {[
              { title: "已暂存", items: staged, staged: true },
              { title: "更改", items: unstaged, staged: false },
            ]
              .filter((section) => section.items.length)
              .map((section) => (
                <section key={section.title}>
                  <h2>
                    <span>{section.title}</span>
                    <small>{section.items.length}</small>
                    <button
                      className="git-section-action"
                      disabled={busy}
                      onClick={() => toggle(section.items, section.staged)}
                    >
                      {section.staged ? (
                        <Minus size={12} />
                      ) : (
                        <Plus size={12} />
                      )}
                      {section.staged ? "全部取消暂存" : "暂存全部"}
                    </button>
                  </h2>
                  {section.items.map((item) => (
                    <div className="git-change-row" key={item.path}>
                      <button className="git-change-open" title={item.path}>
                        <FileCode2 size={15} />
                        <span className="git-change-name">
                          <span
                            className={`git-file git-file-${item.letter.toLowerCase()}`}
                          >
                            {name(item.path)}
                          </span>
                          <small>{item.path.slice(root.length + 1)}</small>
                        </span>
                      </button>
                      <span
                        className={`git-change-badge git-file-${item.letter.toLowerCase()}`}
                        title={item.label}
                      >
                        {item.code === "??"
                          ? "U"
                          : section.staged
                            ? item.code[0]
                            : item.code[1]}
                      </span>
                      <button
                        className="git-file-action"
                        aria-label={`${section.staged ? "取消暂存" : "暂存"} ${name(item.path)}`}
                        title={section.staged ? "取消暂存" : "暂存"}
                        disabled={busy}
                        onClick={() => toggle([item], section.staged)}
                      >
                        {section.staged ? (
                          <Minus size={14} />
                        ) : (
                          <Plus size={14} />
                        )}
                      </button>
                    </div>
                  ))}
                </section>
              ))}
          </div>
        </>
      )}

      {commitOpen && (
        <div className="git-sheet" role="dialog" aria-label="提交修改">
          <form
            className="git-sheet-card git-commit-card"
            onSubmit={(event) => {
              event.preventDefault();
              if (
                !busy &&
                staged.length &&
                message.trim() &&
                identity.name &&
                identity.email
              )
                commit();
            }}
          >
            <header>
              <strong>提交修改</strong>
              <button
                type="button"
                aria-label="关闭"
                onClick={() => setCommitOpen(false)}
              >
                <X size={16} />
              </button>
            </header>
            <div className="git-commit-form">
              <label htmlFor="git-message">提交说明 / comment</label>
              <textarea
                id="git-message"
                aria-label="提交说明"
                placeholder="提交说明"
                value={message}
                required
                rows={4}
                onChange={(event) => setMessage(event.target.value)}
              />
              <fieldset className="git-identity">
                <legend>提交身份</legend>
                <div>
                  <label>
                    姓名
                    <input
                      aria-label="提交者姓名"
                      value={identity.name}
                      required
                      placeholder="姓名"
                      onChange={(event) =>
                        setIdentity({ ...identity, name: event.target.value })
                      }
                    />
                  </label>
                  <label>
                    邮箱
                    <input
                      aria-label="提交者邮箱"
                      type="email"
                      value={identity.email}
                      required
                      placeholder="you@example.com"
                      onChange={(event) =>
                        setIdentity({ ...identity, email: event.target.value })
                      }
                    />
                  </label>
                </div>
              </fieldset>
              {error && (
                <p className="git-panel-error" role="alert">
                  {error}
                </p>
              )}
              <button
                className="primary"
                type="submit"
                disabled={
                  busy ||
                  !staged.length ||
                  !message.trim() ||
                  !identity.name ||
                  !identity.email
                }
              >
                <GitCommitHorizontal size={15} />
                {staged.length
                  ? `提交 ${staged.length} 个文件`
                  : "没有已暂存的文件"}
              </button>
            </div>
          </form>
        </div>
      )}

      {pickerOpen && (
        <div className="git-sheet" role="dialog" aria-label="选择 commit">
          <div className="git-sheet-card git-history-card">
            <header>
              <strong>选择 commit</strong>
              <button
                type="button"
                aria-label="关闭"
                onClick={() => setPickerOpen(false)}
              >
                <X size={16} />
              </button>
            </header>
            <div className="git-history-list">
              {!history.length && (
                <p className="git-panel-empty">暂无提交记录</p>
              )}
              {history.map((item) => {
                const current =
                  item.id === refs?.head ||
                  item.shortId === refs?.head ||
                  Boolean(refs?.head?.startsWith(item.shortId));
                return (
                  <button
                    key={item.id}
                    className={`git-history-item${current ? " active" : ""}`}
                    disabled={busy}
                    onClick={() => {
                      setPickerOpen(false);
                      openCommit(item.id);
                    }}
                  >
                    <span className="git-history-marker">
                      <GitCommitHorizontal size={17} />
                    </span>
                    <span>
                      <strong>{item.subject}</strong>
                      <small>
                        {item.author} ·{" "}
                        {new Date(item.timestamp * 1000).toLocaleString(
                          "zh-CN",
                        )}
                      </small>
                      <code>
                        {item.shortId}
                        {current && (
                          <span className="git-head-badge">当前</span>
                        )}
                      </code>
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      )}

      {remoteOpen && (
        <div className="git-sheet" role="dialog" aria-label="远程仓库">
          <div className="git-sheet-card git-remote-card">
            <header>
              <strong>
                {remoteEditing
                  ? remoteEditing.mode === "new"
                    ? "添加远程仓库"
                    : "编辑远程仓库"
                  : "远程仓库"}
              </strong>
              <button
                type="button"
                aria-label="关闭"
                onClick={() => setRemoteOpen(false)}
              >
                <X size={16} />
              </button>
            </header>
            {remoteEditing ? (
              <form
                className="git-remote-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  saveRemote(remoteEditing.name, remoteEditing.url);
                }}
              >
                <label>
                  名称
                  <input
                    aria-label="远程名称"
                    value={remoteEditing.name}
                    required
                    placeholder="origin"
                    onChange={(event) =>
                      setRemoteEditing({
                        ...remoteEditing,
                        name: event.target.value,
                      })
                    }
                  />
                </label>
                <label>
                  仓库地址
                  <input
                    aria-label="仓库地址"
                    value={remoteEditing.url}
                    required
                    placeholder="https://… / git@…"
                    onChange={(event) =>
                      setRemoteEditing({
                        ...remoteEditing,
                        url: event.target.value,
                      })
                    }
                  />
                </label>
                <div className="git-remote-form-actions">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => setRemoteEditing(null)}
                  >
                    返回
                  </button>
                  <button
                    className="primary"
                    type="submit"
                    disabled={
                      busy ||
                      !remoteEditing.name.trim() ||
                      !remoteEditing.url.trim()
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
                          {remotePendingRemove === item.name ? (
                            <div className="git-remote-confirm">
                              <span>移除 {item.name}？</span>
                              <button
                                onClick={() => setRemotePendingRemove(null)}
                              >
                                取消
                              </button>
                              <button
                                className="git-danger"
                                disabled={busy}
                                onClick={() => removeRemote(item.name)}
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
                                  setRemoteName(item.name);
                                }}
                              >
                                <Cloud size={14} />
                                <span className="git-remote-name">
                                  {item.name}
                                </span>
                                <span
                                  className="git-remote-url"
                                  title={item.url}
                                >
                                  {item.url}
                                </span>
                                {item.name === remoteName && (
                                  <Check
                                    size={13}
                                    className="git-remote-check"
                                  />
                                )}
                              </button>
                              <button
                                className="icon-button"
                                title={`编辑 ${item.name}`}
                                aria-label={`编辑 ${item.name}`}
                                disabled={busy}
                                onClick={() =>
                                  setRemoteEditing({
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
                                onClick={() =>
                                  setRemotePendingRemove(item.name)
                                }
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
                      setRemoteEditing({ mode: "new", name: "", url: "" })
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
                      onChange={setSyncBranch}
                    />
                  </label>
                  <button
                    disabled={busy || !remoteName}
                    onClick={() => sync(false)}
                  >
                    <CloudDownload size={14} /> 拉取
                  </button>
                  <button
                    disabled={busy || !remoteName}
                    onClick={() => sync(true)}
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
      )}
    </div>
  );
}
