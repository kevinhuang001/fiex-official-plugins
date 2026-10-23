import { useEffect, useRef, useState } from "react";
import {
  Archive,
  Check,
  ChevronDown,
  FolderOpen,
  LoaderCircle,
} from "lucide-react";
import { FiexUiClient } from "../../../../sdk/ui/src/index";

interface Context {
  selectionPaths?: string[];
  directoryPath?: string;
}
interface Format {
  id: string;
  label: string;
  ext: string;
  levelMax: number;
  password: boolean;
  note: string;
}

const client = new FiexUiClient();
const formats: Format[] = [
  {
    id: "zip",
    label: "ZIP",
    ext: "zip",
    levelMax: 9,
    password: true,
    note: "Deflate · 支持 AES-256 密码",
  },
  {
    id: "rar",
    label: "RAR",
    ext: "rar",
    levelMax: 5,
    password: true,
    note: "RAR5 · 支持密码",
  },
  {
    id: "7z",
    label: "7Z",
    ext: "7z",
    levelMax: 9,
    password: false,
    note: "LZMA2 · 高压缩比",
  },
  {
    id: "tar",
    label: "TAR",
    ext: "tar",
    levelMax: 0,
    password: false,
    note: "仅打包，不压缩",
  },
  {
    id: "tar.gz",
    label: "TAR.GZ",
    ext: "tar.gz",
    levelMax: 9,
    password: false,
    note: "Gzip 压缩",
  },
  {
    id: "tar.bz2",
    label: "TAR.BZ2",
    ext: "tar.bz2",
    levelMax: 9,
    password: false,
    note: "Bzip2 压缩",
  },
  {
    id: "tar.xz",
    label: "TAR.XZ",
    ext: "tar.xz",
    levelMax: 9,
    password: false,
    note: "XZ 压缩",
  },
];
const lastSegment = (value: string) => value.split(/[/\\]/).pop() || value;
const trimSlash = (value: string) => value.replace(/[\\/]+$/, "");

function FormatSelect({
  value,
  onChange,
  hero = false,
}: {
  value: string;
  onChange: (value: string) => void;
  hero?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  const current = formats.find((entry) => entry.id === value) || formats[0];
  return (
    <div className="format-select" ref={root}>
      <button
        type="button"
        className={`format-trigger${hero ? " hero" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="压缩格式"
        onClick={() => setOpen(!open)}
      >
        {hero && (
          <span className="format-hero-icon">
            <Archive size={18} />
          </span>
        )}
        <span className="format-trigger-copy">
          <strong>{current.label}</strong>
        </span>
        <ChevronDown size={14} className={open ? "open" : ""} />
      </button>
      {open && (
        <div className="format-menu" role="listbox" aria-label="压缩格式">
          {formats.map((entry) => (
            <button
              type="button"
              role="option"
              aria-selected={entry.id === value}
              className={`format-option ${entry.id === value ? "active" : ""}`}
              key={entry.id}
              onClick={() => {
                onChange(entry.id);
                setOpen(false);
              }}
            >
              <span className="format-option-copy">
                <strong>{entry.label}</strong>
              </span>
              {entry.id === value && <Check size={13} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function App() {
  const appRef = useRef<HTMLDivElement>(null);
  const [paths, setPaths] = useState<string[]>([]);
  const [directory, setDirectory] = useState("");
  const [ready, setReady] = useState(false);
  const [format, setFormat] = useState("zip");
  const [level, setLevel] = useState(6);
  const [password, setPassword] = useState("");
  const [compressAll, setCompressAll] = useState(false);
  const [destination, setDestination] = useState("");
  const [editedDestination, setEditedDestination] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [done, setDone] = useState("");
  const selected = formats.find((entry) => entry.id === format) || formats[0];
  const base =
    paths.length === 1
      ? lastSegment(paths[0]).replace(/\.[^.]+$/, "")
      : "Archive";

  useEffect(() => {
    void client
      .invoke<Context>("context.get")
      .then((context) => {
        setPaths(context.selectionPaths || []);
        setDirectory(context.directoryPath || "");
      })
      .catch((cause) => setError(String(cause)))
      .finally(() => setReady(true));
  }, []);
  useEffect(() => {
    if (editedDestination) return;
    const folder = trimSlash(directory);
    setDestination(folder ? `${folder}/${base}.${selected.ext}` : "");
  }, [directory, base, selected.ext, editedDestination]);
  useEffect(() => {
    setLevel((current) => Math.min(current, selected.levelMax));
  }, [selected.levelMax]);
  useEffect(() => {
    const app = appRef.current;
    const content = app?.querySelector(
      ".archive-content",
    ) as HTMLElement | null;
    const footer = app?.querySelector("footer") as HTMLElement | null;
    if (!app || !content || !footer) return;
    const style = getComputedStyle(app);
    const padding =
      parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    const gap = parseFloat(style.rowGap || style.gap) || 12;
    const height = Math.ceil(
      padding + content.scrollHeight + gap + footer.offsetHeight,
    );
    void client
      .invoke("panel.resize", { width: 400, height: Math.max(260, height) })
      .catch(() => {});
  }, [paths, format, password, error, done, ready]);

  const pickDestination = async () => {
    setError("");
    try {
      const picked = await client.dialog.saveFile({
        suggestedName: `${base}.${selected.ext}`,
        extensions: [selected.ext.split(".").pop() || "zip"],
        directory: directory || undefined,
      });
      if (picked) {
        setDestination(picked);
        setEditedDestination(true);
      }
    } catch (cause) {
      setError(String(cause));
    }
  };
  const compress = async () => {
    if (!paths.length) return;
    if (!destination.trim()) {
      setError("请选择压缩到的位置");
      return;
    }
    setBusy(true);
    setError("");
    setDone("");
    let task: { taskId: string } | null = null;
    try {
      task = await client.tasks.create(`正在压缩为 ${selected.label}`, {
        detail: `${paths.length} 个项目`,
      });
      const created = await client.invoke<string>("plugin.invoke", {
        method: "archive.compress",
        params: {
          paths,
          destination: destination.trim(),
          format: selected.id,
          level: password ? null : level,
          password: password || null,
          ignoreSettings: !compressAll,
        },
      });
      setDone(created);
      await client.tasks.finish(task.taskId, { detail: created });
    } catch (cause) {
      setError(String(cause));
      if (task)
        await client.tasks
          .finish(task.taskId, { error: String(cause) })
          .catch(() => {});
    } finally {
      setBusy(false);
    }
  };

  if (!ready) {
    return (
      <div className="app">
        <div className="archive-loading">
          <LoaderCircle size={18} className="spin" />
        </div>
      </div>
    );
  }
  if (!paths.length) {
    return (
      <div className="app">
        <div className="archive-loading">请选择要压缩的文件或文件夹。</div>
      </div>
    );
  }

  return (
    <div className="app" ref={appRef}>
      <div className="archive-content">
        <FormatSelect
          hero
          value={format}
          onChange={(next) => {
            setFormat(next);
            setPassword("");
          }}
        />

        <div className="archive-field">
          <span className="archive-label">压缩到</span>
          <div className="archive-path-row">
            <span className="archive-path" title={destination || "未选择"}>
              {destination || "未选择"}
            </span>
            <button
              type="button"
              className="archive-pick"
              onClick={() => void pickDestination()}
            >
              <FolderOpen size={14} /> 选择…
            </button>
          </div>
        </div>

        {selected.levelMax > 0 && (
          <div className="archive-field">
            <span className="archive-label">压缩级别</span>
            <div
              className="archive-levels"
              role="radiogroup"
              aria-label="压缩级别"
            >
              {Array.from({ length: selected.levelMax + 1 }, (_, value) => (
                <button
                  type="button"
                  role="radio"
                  aria-checked={value === level}
                  className={value === level ? "active" : ""}
                  key={value}
                  onClick={() => setLevel(value)}
                >
                  {value}
                </button>
              ))}
            </div>
          </div>
        )}

        {selected.password && (
          <label className="archive-field">
            <span className="archive-label">密码（可选）</span>
            <input
              type="password"
              className="archive-input"
              value={password}
              autoComplete="off"
              placeholder="留空则不加密"
              onChange={(event) => setPassword(event.target.value)}
            />
          </label>
        )}

        <button
          type="button"
          role="checkbox"
          aria-checked={compressAll}
          className={`archive-check${compressAll ? " checked" : ""}`}
          onClick={() => setCompressAll(!compressAll)}
        >
          <span className="archive-check-box">
            {compressAll && <Check size={12} />}
          </span>
          全部压缩
        </button>
        {error && <div className="error">{error}</div>}
        {done && <div className="done">已创建 {done}</div>}
      </div>
      <footer>
        <button onClick={() => void client.invoke("panel.close")}>取消</button>
        <button
          className="primary"
          disabled={busy}
          onClick={() => void compress()}
        >
          {busy ? (
            <LoaderCircle size={14} className="spin" />
          ) : (
            <Archive size={14} />
          )}
          开始压缩
        </button>
      </footer>
    </div>
  );
}
