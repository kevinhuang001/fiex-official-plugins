import type { Dispatch, SetStateAction } from "react";
import { client } from "../client";
import { baseName } from "../paths";
import type { CreateKeyInput, Key, Status } from "../types";

/** Import, export, create and delete key operations for the keychain. Status
 *  is shared with the panel footer; creation/deletion refresh the key list. */
export function useKeyActions({
  directory,
  refreshKeys,
  setStatus,
}: {
  directory: string;
  refreshKeys: () => Promise<void>;
  setStatus: Dispatch<SetStateAction<Status>>;
}) {
  const imports = async () => {
    const path = await client.dialog.openFile({
      directory,
      extensions: ["asc", "gpg", "pgp", "key"],
    });
    if (!path) return;
    setStatus({ kind: "busy", text: "正在导入密钥…" });
    try {
      const result = await client.plugin.invoke<{ imported: number }>(
        "gpg.import",
        {
          input: path,
        },
      );
      await refreshKeys();
      setStatus({ kind: "ok", text: `已导入 ${result.imported} 个密钥` });
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

  const exportSecret = async (key: Key) => {
    const path = await client.dialog.saveFile({
      directory,
      suggestedName: `${key.keyId || "secret-key"}-secret.asc`,
      extensions: ["asc"],
    });
    if (!path) return;
    setStatus({ kind: "busy", text: "正在导出私钥…" });
    try {
      await client.plugin.invoke("gpg.export", {
        output: path,
        fingerprint: key.fingerprint,
        armor: true,
        secret: true,
      });
      setStatus({ kind: "ok", text: `已导出私钥到 ${baseName(path)}` });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
    }
  };

  const createKey = async (input: CreateKeyInput) => {
    setStatus({ kind: "busy", text: "正在创建密钥…" });
    try {
      const result = await client.plugin.invoke<{ fingerprint?: string }>(
        "gpg.createKey",
        input,
      );
      await refreshKeys();
      setStatus({
        kind: "ok",
        text: result.fingerprint
          ? `已创建密钥 ${result.fingerprint.slice(-8)}`
          : "密钥已创建",
      });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
      throw error;
    }
  };

  const deleteKey = async (key: Key) => {
    setStatus({ kind: "busy", text: "正在删除密钥…" });
    try {
      await client.plugin.invoke("gpg.deleteKey", {
        fingerprint: key.fingerprint,
        secret: Boolean(key.hasSecret),
      });
      await refreshKeys();
      setStatus({ kind: "ok", text: "密钥已删除" });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
      throw error;
    }
  };

  return { imports, exports, exportSecret, createKey, deleteKey };
}
