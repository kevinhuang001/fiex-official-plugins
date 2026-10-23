import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  FileSignature,
  KeyRound,
  Lock,
  LockOpen,
  RefreshCw,
  Search,
  ShieldCheck,
  Upload,
} from "lucide-react";
import { FiexUiClient } from "../../../../sdk/ui/src/index";

const client = new FiexUiClient();

type Action = "encrypt" | "decrypt" | "sign" | "verify" | "keys";
type Key = {
  fingerprint: string;
  keyId: string;
  name: string;
  email: string;
  expires: string;
};
type Status = { kind: "idle" | "busy" | "ok" | "error"; text: string };

const ACTIONS: Record<
  Action,
  { label: string; heading: string; icon: typeof Lock; width: number }
> = {
  encrypt: {
    label: "加密",
    heading: "用公钥加密所选文件",
    icon: Lock,
    width: 540,
  },
  decrypt: {
    label: "解密",
    heading: "用私钥解密所选文件",
    icon: LockOpen,
    width: 500,
  },
  sign: {
    label: "签名",
    heading: "生成分离签名",
    icon: FileSignature,
    width: 520,
  },
  verify: {
    label: "验证",
    heading: "核验分离签名",
    icon: ShieldCheck,
    width: 500,
  },
  keys: { label: "公钥", heading: "本机公钥环", icon: KeyRound, width: 420 },
};

function baseName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function directoryName(path: string) {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index > 0 ? path.slice(0, index) : path;
}

function join(directory: string, name: string) {
  const separator = directory.includes("\\") ? "\\" : "/";
  return `${directory.replace(/[\\/]+$/, "")}${separator}${name}`;
}

function withoutSignature(path: string) {
  const lower = path.toLowerCase();
  for (const suffix of [".sig.asc", ".sig", ".asc"]) {
    if (lower.endsWith(suffix)) return path.slice(0, -suffix.length);
  }
  return path;
}

function isSignature(path: string) {
  const lower = path.toLowerCase();
  return lower.endsWith(".sig") || lower.endsWith(".asc");
}

function outputName(input: string, action: Action, armor: boolean) {
  const name = baseName(input);
  const lower = name.toLowerCase();
  if (action === "encrypt") {
    return lower.endsWith(".gpg") || lower.endsWith(".asc")
      ? name
      : name + (armor ? ".asc" : ".gpg");
  }
  if (action === "decrypt") {
    for (const suffix of [".gpg", ".pgp", ".asc"]) {
      if (lower.endsWith(suffix)) return name.slice(0, -suffix.length);
    }
    return name + ".decrypted";
  }
  if (action === "sign") return name + (armor ? ".sig.asc" : ".sig");
  return name;
}

export function App() {
  const [action, setAction] = useState<Action>("encrypt");
  const [files, setFiles] = useState<string[]>([]);
  const [directory, setDirectory] = useState("");
  const [signature, setSignature] = useState("");
  const [data, setData] = useState("");
  const [keys, setKeys] = useState<Key[]>([]);
  const [fingerprint, setFingerprint] = useState("");
  const [query, setQuery] = useState("");
  const [armor, setArmor] = useState(true);
  const [trust, setTrust] = useState(true);
  const [status, setStatus] = useState<Status>({ kind: "idle", text: "" });
  const [progress, setProgress] = useState(0);
  const [gpg, setGpg] = useState("");
  const loaded = useRef(false);

  const busy = status.kind === "busy";
  const meta = ACTIONS[action];
  const visibleKeys = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return keys;
    return keys.filter(
      (key) =>
        key.name.toLowerCase().includes(needle) ||
        key.email.toLowerCase().includes(needle) ||
        key.fingerprint.toLowerCase().includes(needle),
    );
  }, [keys, query]);
  const selectedKeys = useMemo(
    () => keys.filter((key) => key.fingerprint === fingerprint),
    [keys, fingerprint],
  );

  useEffect(() => {
    if (loaded.current) return;
    loaded.current = true;
    void (async () => {
      try {
        const context = await client.context.get();
        const selection = context.selectionPaths || [];
        const requested = (context.panelId || "").split(".").pop() as
          Action | undefined;
        const next = requested && requested in ACTIONS ? requested : "encrypt";
        setAction(next);
        setDirectory(context.directoryPath || "");
        if (next === "verify") {
          const signed = selection.find(isSignature);
          setSignature(signed || "");
          setData(signed ? withoutSignature(signed) : selection[0] || "");
          setFiles(selection.slice(0, 1));
        } else {
          setFiles(selection);
          setData(selection[0] || "");
        }
        const version = await client.plugin
          .invoke<{ version: string }>("gpg.info")
          .then((value) => value.version)
          .catch(() => "");
        setGpg(version);
        if (version) {
          const list = await client.plugin.invoke<Key[]>("gpg.keys");
          setKeys(list);
          setFingerprint(list[0]?.fingerprint || "");
        }
      } catch (error) {
        setStatus({ kind: "error", text: String(error) });
      }
    })();
  }, []);

  // Size the sheet to its content instead of the manifest's worst case: the
  // host clamps the request and keeps the title bar outside this height.
  useEffect(() => {
    const root = document.querySelector(".gpg") as HTMLElement | null;
    if (!root) return;
    const previous = root.style.height;
    root.style.height = "auto";
    const measured = Math.ceil(root.scrollHeight);
    root.style.height = previous;
    void client.panel
      .resize(meta.width, Math.max(300, measured))
      .catch(() => {});
  }, [meta.width, keys, files, signature, data, status]);

  const refreshKeys = async () => {
    const list = await client.plugin.invoke<Key[]>("gpg.keys");
    setKeys(list);
    if (!list.some((key) => key.fingerprint === fingerprint)) {
      setFingerprint(list[0]?.fingerprint || "");
    }
  };

  const pickSignature = async () => {
    const path = await client.dialog.openFile({ directory });
    if (path) {
      setSignature(path);
      if (!data) setData(withoutSignature(path));
    }
  };

  const pickData = async () => {
    const path = await client.dialog.openFile({ directory });
    if (path) setData(path);
  };

  const imports = async () => {
    const path = await client.dialog.openFile({
      directory,
      extensions: ["asc", "gpg", "pgp"],
    });
    if (!path) return;
    setStatus({ kind: "busy", text: "正在导入公钥…" });
    try {
      const result = await client.plugin.invoke<{ imported: number }>(
        "gpg.import",
        { input: path },
      );
      await refreshKeys();
      setStatus({ kind: "ok", text: `已导入 ${result.imported} 个公钥` });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
    }
  };

  const exports = async (key: Key) => {
    const path = await client.dialog.saveFile({
      directory,
      suggestedName: `${key.keyId || "public-key"}.asc`,
      extensions: ["asc"],
    });
    if (!path) return;
    setStatus({ kind: "busy", text: "正在导出公钥…" });
    try {
      await client.plugin.invoke("gpg.export", {
        output: path,
        fingerprint: key.fingerprint,
        armor: true,
      });
      setStatus({ kind: "ok", text: `已导出到 ${baseName(path)}` });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
    }
  };

  const run = async () => {
    const inputs = (action === "verify" ? [data] : files).filter(Boolean);
    if (!inputs.length) {
      setStatus({ kind: "error", text: "请选择要处理的文件" });
      return;
    }
    if (action === "verify" && !signature) {
      setStatus({ kind: "error", text: "请选择签名文件" });
      return;
    }
    if ((action === "encrypt" || action === "sign") && !fingerprint) {
      setStatus({ kind: "error", text: "请选择密钥" });
      return;
    }
    setStatus({ kind: "busy", text: "" });
    setProgress(0);
    let done = 0;
    const failures: string[] = [];
    for (const input of inputs) {
      setStatus({ kind: "busy", text: `正在处理 ${baseName(input)}` });
      try {
        if (action === "verify") {
          const result = await client.plugin.invoke<{
            signer: string;
            fingerprint: string;
          }>("gpg.verify", { signature, input });
          setStatus({
            kind: "ok",
            text: `签名有效${result.signer ? ` · 签名者 ${result.signer}` : ""}`,
          });
          done += 1;
          continue;
        }
        const output = join(
          directoryName(input),
          outputName(input, action, armor),
        );
        await client.plugin.invoke(`gpg.${action}`, {
          input,
          output,
          fingerprint,
          armor,
          trustUnverified: trust,
        });
        done += 1;
      } catch (error) {
        failures.push(`${baseName(input)}：${String(error)}`);
      }
      setProgress(done);
    }
    if (failures.length) {
      setStatus({ kind: "error", text: failures[0] });
    } else if (action !== "verify") {
      setStatus({ kind: "ok", text: `已完成 ${done} 个文件` });
    }
  };

  const canRun =
    !busy &&
    gpg !== "" &&
    (action === "verify"
      ? Boolean(signature && data)
      : files.length > 0 && (action === "decrypt" || Boolean(fingerprint)));

  return (
    <main className="gpg">
      <header className="gpg-head">
        <div className="gpg-title">
          <span className="gpg-mark" aria-hidden>
            <meta.icon size={15} />
          </span>
          <div>
            <h1>{meta.label}</h1>
            <p>{meta.heading}</p>
          </div>
        </div>
        {action !== "keys" && files.length > 0 && (
          <span className="gpg-count">{files.length} 个文件</span>
        )}
      </header>

      <section className="gpg-body">
        {action === "keys" ? (
          <KeysView
            keys={visibleKeys}
            total={keys.length}
            query={query}
            onQuery={setQuery}
            onExport={exports}
            onImport={imports}
            onRefresh={() => void refreshKeys()}
            busy={busy}
          />
        ) : action === "verify" ? (
          <>
            <Field label="签名文件">
              <PathInput
                value={signature}
                onPick={pickSignature}
                disabled={busy}
              />
            </Field>
            <Field label="被签名的文件">
              <PathInput value={data} onPick={pickData} disabled={busy} />
            </Field>
            <p className="gpg-hint">
              支持分离签名（.sig / .asc），签名者由本机公钥环验证。
            </p>
          </>
        ) : (
          <>
            <FileList files={files} />
            <div className="gpg-out">
              <span>输出</span>
              <code>{outputName(files[0] || "文件", action, armor)}</code>
              <small>
                {directory ? `保存到 ${directory}` : "保存到原目录"}
              </small>
            </div>
            {action !== "decrypt" && (
              <KeyPicker
                keys={visibleKeys}
                total={keys.length}
                value={fingerprint}
                onChange={setFingerprint}
                query={query}
                onQuery={setQuery}
                busy={busy}
              />
            )}
            <div className="gpg-options">
              {(action === "encrypt" || action === "sign") && (
                <Toggle
                  label={
                    action === "sign" ? "ASCII 装甲签名" : "ASCII 装甲输出"
                  }
                  checked={armor}
                  onChange={setArmor}
                  disabled={busy}
                />
              )}
              {action === "encrypt" && (
                <Toggle
                  label="允许未验证的收件人密钥"
                  checked={trust}
                  onChange={setTrust}
                  disabled={busy}
                />
              )}
            </div>
            {action === "encrypt" && selectedKeys[0] && (
              <p className="gpg-hint">
                收件人指纹 {selectedKeys[0].fingerprint}
              </p>
            )}
            {action === "decrypt" && (
              <p className="gpg-hint">
                私钥口令由系统 pinentry 窗口输入，本面板不会读取。
              </p>
            )}
          </>
        )}
      </section>

      <footer className="gpg-foot">
        <div className={`gpg-status is-${status.kind}`} role="status">
          {status.kind === "busy" && <span className="gpg-spin" aria-hidden />}
          {status.text || (gpg ? "就绪" : "需要安装 GnuPG 才能使用此插件")}
          {busy && files.length > 1 && (
            <em>
              {progress}/{files.length}
            </em>
          )}
        </div>
        {action !== "keys" && (
          <button
            className="gpg-primary"
            disabled={!canRun}
            onClick={() => void run()}
          >
            {busy ? "处理中…" : meta.label}
          </button>
        )}
      </footer>
    </main>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="gpg-field">
      <span>{label}</span>
      {children}
    </label>
  );
}

function PathInput({
  value,
  onPick,
  disabled,
}: {
  value: string;
  onPick: () => void;
  disabled: boolean;
}) {
  return (
    <div className="gpg-path">
      <input value={value} readOnly placeholder="尚未选择文件" />
      <button type="button" onClick={onPick} disabled={disabled}>
        选择
      </button>
    </div>
  );
}

function FileList({ files }: { files: string[] }) {
  if (!files.length) return <p className="gpg-empty">没有选中的文件</p>;
  return (
    <ul className="gpg-files">
      {files.slice(0, 4).map((file) => (
        <li key={file} title={file}>
          <FileSignature size={13} />
          <span>{baseName(file)}</span>
        </li>
      ))}
      {files.length > 4 && <li className="gpg-more">+{files.length - 4}</li>}
    </ul>
  );
}

function KeyPicker({
  keys,
  total,
  value,
  onChange,
  query,
  onQuery,
  busy,
}: {
  keys: Key[];
  total: number;
  value: string;
  onChange: (value: string) => void;
  query: string;
  onQuery: (value: string) => void;
  busy: boolean;
}) {
  if (!total) {
    return (
      <div className="gpg-empty">
        <KeyRound size={15} />
        没有可用公钥。请从工具栏的“公钥管理”导入密钥。
      </div>
    );
  }
  return (
    <div className="gpg-keys">
      <div className="gpg-keys-head">
        <span>收件人</span>
        {total > 3 && (
          <div className="gpg-search">
            <Search size={12} />
            <input
              value={query}
              onChange={(event) => onQuery(event.target.value)}
              placeholder="搜索"
              disabled={busy}
            />
          </div>
        )}
      </div>
      <ul className="gpg-key-list">
        {keys.slice(0, 6).map((key) => (
          <li key={key.fingerprint}>
            <button
              type="button"
              aria-pressed={value === key.fingerprint}
              disabled={busy}
              onClick={() => onChange(key.fingerprint)}
            >
              <span className="gpg-radio" aria-hidden />
              <span className="gpg-key-text">
                <strong>{key.name || key.email || key.keyId}</strong>
                <small>{key.fingerprint.slice(-16)}</small>
              </span>
            </button>
          </li>
        ))}
        {keys.length === 0 && <li className="gpg-empty">没有匹配的密钥</li>}
      </ul>
    </div>
  );
}

function KeysView({
  keys,
  total,
  query,
  onQuery,
  onExport,
  onImport,
  onRefresh,
  busy,
}: {
  keys: Key[];
  total: number;
  query: string;
  onQuery: (value: string) => void;
  onExport: (key: Key) => void;
  onImport: () => void;
  onRefresh: () => void;
  busy: boolean;
}) {
  return (
    <>
      <div className="gpg-keys-head">
        <span>{total} 个公钥</span>
        <div className="gpg-actions">
          <button type="button" onClick={onImport} disabled={busy}>
            <Upload size={13} />
            导入
          </button>
          <button type="button" onClick={onRefresh} disabled={busy}>
            <RefreshCw size={13} />
            刷新
          </button>
        </div>
      </div>
      {total > 4 && (
        <div className="gpg-search wide">
          <Search size={12} />
          <input
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder="搜索名称、邮箱或指纹"
            disabled={busy}
          />
        </div>
      )}
      <ul className="gpg-key-list full">
        {keys.map((key) => (
          <li key={key.fingerprint} className="gpg-key-row">
            <div className="gpg-key-text">
              <strong>{key.name || key.email || key.keyId}</strong>
              <small>{key.fingerprint}</small>
            </div>
            <button type="button" onClick={() => onExport(key)} disabled={busy}>
              导出
            </button>
          </li>
        ))}
        {keys.length === 0 && <li className="gpg-empty">没有匹配的公钥</li>}
      </ul>
    </>
  );
}

function Toggle({
  label,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled: boolean;
}) {
  return (
    <label className="gpg-toggle">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span aria-hidden />
      {label}
    </label>
  );
}
