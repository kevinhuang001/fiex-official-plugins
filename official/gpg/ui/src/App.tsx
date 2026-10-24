import { useEffect, useMemo, useRef, useState } from "react";
import { ActionFooter } from "./components/ActionFooter";
import { DecryptForm } from "./components/DecryptForm";
import { EncryptForm } from "./components/EncryptForm";
import { Header } from "./components/Header";
import { KeysView } from "./components/KeysView";
import { SignForm } from "./components/SignForm";
import { VerifyForm } from "./components/VerifyForm";
import { useGpgAction } from "./hooks/useGpgAction";
import { useGpgKeys } from "./hooks/useGpgKeys";
import { baseName, isSignature, withoutSignature } from "./paths";
import { client } from "./client";
import {
  ACTIONS,
  DEFAULT_ACTION,
  isAction,
  type Action,
  type Key,
  type Status,
} from "./types";

export function App() {
  const [action, setAction] = useState<Action>(DEFAULT_ACTION);
  const [files, setFiles] = useState<string[]>([]);
  const [directory, setDirectory] = useState("");
  const [signature, setSignature] = useState("");
  const [data, setData] = useState("");
  const [query, setQuery] = useState("");
  const [armor, setArmor] = useState(true);
  const [trust, setTrust] = useState(true);
  const [status, setStatus] = useState<Status>({ kind: "idle", text: "" });
  const loaded = useRef(false);

  const { keys, fingerprint, setFingerprint, gpg, loadKeys, refreshKeys } =
    useGpgKeys();
  const { progress, run } = useGpgAction({
    action,
    files,
    data,
    signature,
    fingerprint,
    armor,
    trust,
    setStatus,
  });

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
        const requested = (context.panelId || "").split(".").pop();
        const next = isAction(requested) ? requested : DEFAULT_ACTION;
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
        await loadKeys();
      } catch (error) {
        setStatus({ kind: "error", text: String(error) });
      }
    })();
  }, [loadKeys]);

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

  const canRun =
    !busy &&
    gpg !== "" &&
    (action === "verify"
      ? Boolean(signature && data)
      : files.length > 0 && (action === "decrypt" || Boolean(fingerprint)));

  return (
    <main className="gpg">
      <Header
        meta={meta}
        showCount={action !== "keys" && files.length > 0}
        count={files.length}
      />

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
          <VerifyForm
            signature={signature}
            data={data}
            onPickSignature={pickSignature}
            onPickData={pickData}
            busy={busy}
          />
        ) : action === "encrypt" ? (
          <EncryptForm
            files={files}
            directory={directory}
            keys={visibleKeys}
            totalKeys={keys.length}
            fingerprint={fingerprint}
            onFingerprint={setFingerprint}
            selectedKey={selectedKeys[0]}
            query={query}
            onQuery={setQuery}
            busy={busy}
            armor={armor}
            onArmor={setArmor}
            trust={trust}
            onTrust={setTrust}
          />
        ) : action === "decrypt" ? (
          <DecryptForm files={files} directory={directory} armor={armor} />
        ) : (
          <SignForm
            files={files}
            directory={directory}
            keys={visibleKeys}
            totalKeys={keys.length}
            fingerprint={fingerprint}
            onFingerprint={setFingerprint}
            query={query}
            onQuery={setQuery}
            busy={busy}
            armor={armor}
            onArmor={setArmor}
          />
        )}
      </section>

      <ActionFooter
        status={status}
        gpg={gpg}
        busy={busy}
        progress={progress}
        fileCount={files.length}
        showAction={action !== "keys"}
        actionLabel={meta.label}
        canRun={canRun}
        onRun={run}
      />
    </main>
  );
}
