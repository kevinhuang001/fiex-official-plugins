import { useCallback, useState } from "react";
import { client } from "../client";
import type { Key } from "../types";

export function useGpgKeys() {
  const [keys, setKeys] = useState<Key[]>([]);
  const [fingerprint, setFingerprint] = useState("");
  const [gpg, setGpg] = useState("");

  const loadKeys = useCallback(async () => {
    const version = await client.plugin
      .invoke<{ version: string }>("gpg.info")
      .then((value) => value.version)
      .catch(() => "");
    setGpg(version);
    if (!version) return;
    const list = await client.plugin.invoke<Key[]>("gpg.keys");
    setKeys(list);
    setFingerprint(list[0]?.fingerprint || "");
  }, []);

  const refreshKeys = useCallback(async () => {
    const list = await client.plugin.invoke<Key[]>("gpg.keys");
    setKeys(list);
    if (!list.some((key) => key.fingerprint === fingerprint)) {
      setFingerprint(list[0]?.fingerprint || "");
    }
  }, [fingerprint]);

  return { keys, fingerprint, setFingerprint, gpg, loadKeys, refreshKeys };
}
