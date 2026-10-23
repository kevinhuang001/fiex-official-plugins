import { useEffect, useState } from "react";
import { FiexUiClient } from "../../../../sdk/ui/src/index";
const client = new FiexUiClient();
type Key = { fingerprint: string; name: string };
export function App() {
  const [mode, setMode] = useState("encrypt");
  const [input, setInput] = useState("");
  const [signature, setSignature] = useState("");
  const [output, setOutput] = useState("");
  const [keys, setKeys] = useState<Key[]>([]);
  const [fingerprint, setFingerprint] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const refresh = async () => {
    const keys = await client.plugin.invoke<Key[]>("gpg.keys");
    setKeys(keys);
    setFingerprint((value) => value || keys[0]?.fingerprint || "");
  };
  useEffect(() => {
    void client.context
      .get()
      .then((ctx) => setInput(ctx.selectionPaths[0] || ""))
      .catch((error) => setMessage(String(error)));
    void refresh().catch((error) => setMessage(String(error)));
  }, []);
  const run = async () => {
    setBusy(true);
    setMessage("");
    try {
      const result = await client.plugin.invoke<{ status: string }>(
        "gpg." + mode,
        { input, output, signature, fingerprint },
      );
      setMessage("操作完成\n" + result.status);
      if (mode === "import") await refresh();
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  };
  const pick = async (setter: (value: string) => void) => {
    const path = await client.dialog.openFile();
    if (path) setter(path);
  };
  const needsKey = ["encrypt", "sign", "export"].includes(mode);
  const needsOutput = ["encrypt", "decrypt", "sign", "export"].includes(mode);
  return (
    <main className="gpg-panel">
      <label>
        操作
        <select
          disabled={busy}
          value={mode}
          onChange={(event) => {
            setMode(event.target.value);
            setOutput("");
            setMessage("");
          }}
        >
          {[
            ["encrypt", "加密文件"],
            ["decrypt", "解密文件"],
            ["sign", "分离签名"],
            ["verify", "验证签名"],
            ["import", "导入公钥"],
            ["export", "导出公钥"],
          ].map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      {mode !== "export" && (
        <label>
          输入文件
          <div className="field-row">
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              disabled={busy}
            />
            <button
              disabled={busy}
              onClick={() =>
                void pick(setInput).catch((e) => setMessage(String(e)))
              }
            >
              选择
            </button>
          </div>
        </label>
      )}
      {mode === "verify" && (
        <label>
          签名文件
          <div className="field-row">
            <input
              value={signature}
              onChange={(e) => setSignature(e.target.value)}
              disabled={busy}
            />
            <button
              disabled={busy}
              onClick={() =>
                void pick(setSignature).catch((e) => setMessage(String(e)))
              }
            >
              选择
            </button>
          </div>
        </label>
      )}
      {needsKey && (
        <label>
          密钥
          <select
            disabled={busy}
            value={fingerprint}
            onChange={(e) => setFingerprint(e.target.value)}
          >
            <option value="">选择密钥</option>
            {keys.map((key, index) => (
              <option key={key.fingerprint + index} value={key.fingerprint}>
                {key.name} · {key.fingerprint.slice(-16)}
              </option>
            ))}
          </select>
        </label>
      )}
      {needsOutput && (
        <label>
          输出文件
          <div className="field-row">
            <input
              value={output}
              onChange={(e) => setOutput(e.target.value)}
              disabled={busy}
            />
            <button
              disabled={busy}
              onClick={() =>
                void client.dialog
                  .saveFile({
                    suggestedName:
                      (input.split(/[\\/]/).pop() || "public-key") +
                      (mode === "encrypt"
                        ? ".gpg"
                        : mode === "decrypt"
                          ? ".decrypted"
                          : ".asc"),
                  })
                  .then((path) => {
                    if (path) setOutput(path);
                  })
                  .catch((e) => setMessage(String(e)))
              }
            >
              选择
            </button>
          </div>
        </label>
      )}
      <p className="hint">
        需要安装 GnuPG。私钥口令由系统 pinentry 窗口输入。输出文件必须尚不存在。
      </p>
      <button
        className="primary"
        disabled={
          busy ||
          (mode !== "export" && !input) ||
          (needsOutput && !output) ||
          (needsKey && !fingerprint) ||
          (mode === "verify" && !signature)
        }
        onClick={() => void run()}
      >
        {busy ? "正在处理…" : "执行"}
      </button>
      <pre role="status">{message}</pre>
    </main>
  );
}
