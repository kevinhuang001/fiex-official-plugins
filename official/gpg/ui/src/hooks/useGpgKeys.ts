import { useCallback, useState } from "react";
import { client } from "../client";
import type { Key } from "../types";

export function useGpgKeys() {
  const [keys, setKeys] = useState<Key[]>([]);
  const [fingerprint, setFingerprint] = useState("");
  const [gpg, setGpg] = useState("");
  const [loading, setLoading] = useState(true);

  const loadKeys = useCallback(async () => {
    setLoading(true);
    try {
      const version = await client.plugin
        .invoke<{ version: string }>("gpg.info")
        .then((value) => value.version)
        .catch(() => "");
      setGpg(version);
      if (!version) return;
      const list = await client.plugin.invoke<Key[]>("gpg.keys");
      setKeys(list);
      setFingerprint(list[0]?.fingerprint || "");
    } finally {
      setLoading(false);
    }
  }, []);

  const refreshKeys = useCallback(async () => {
    setLoading(true);
    try {
      const list = await client.plugin.invoke<Key[]>("gpg.keys");
      setKeys(list);
      if (!list.some((key) => key.fingerprint === fingerprint)) {
        setFingerprint(list[0]?.fingerprint || "");
      }
    } finally {
      setLoading(false);
    }
  }, [fingerprint]);

  return { keys, fingerprint, setFingerprint, gpg, loadKeys, refreshKeys, loading };
}
