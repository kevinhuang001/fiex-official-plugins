import { useCallback, useEffect, useState } from "react";
import type { FiexUiClient } from "../../../../../sdk/ui/src/index";
import type { Commit, Context, Identity, Refs, RemoteStatus, Status } from "../types";
import { invokeGit } from "./useGitClient";

/**
 * Loads the repository snapshot the panel renders: status, refs, history,
 * commit identity and remote tracking state. The host returns these together,
 * so they are fetched in one ordered pass (status first, then the rest in
 * parallel) and share one busy/error lifecycle.
 */
export function useGitRepository(client: FiexUiClient) {
  const [context, setContext] = useState<Context>({});
  const [status, setStatus] = useState<Status | null>(null);
  const [refs, setRefs] = useState<Refs | null>(null);
  const [history, setHistory] = useState<Commit[]>([]);
  const [identity, setIdentity] = useState<Identity>({ name: "", email: "" });
  const [remote, setRemote] = useState<RemoteStatus | null>(null);
  const [remoteName, setRemoteName] = useState("origin");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const root = context.directory?.ref;

  const load = useCallback(async () => {
    if (!root) return;
    setBusy(true);
    setError("");
    try {
      const next = await invokeGit<Status | null>(client, "git.status", { root });
      setStatus(next);
      if (next) {
        const [nextRefs, nextHistory, nextIdentity, nextRemote] =
          await Promise.all([
            invokeGit<Refs>(client, "git.refs", { root }),
            invokeGit<Commit[]>(client, "git.history", { root, skip: 0 }),
            invokeGit<Identity>(client, "git.identity", { root }),
            invokeGit<RemoteStatus>(client, "git.remoteStatus", { root }),
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
  }, [client, root]);

  useEffect(() => {
    void client
      .invoke<Context>("context.get")
      .then((next) => setContext(next))
      .catch((cause) => setError(String(cause)));
  }, [client]);
  useEffect(() => {
    if (root) void load();
  }, [root, load]);

  return {
    root,
    name: context.directory?.metadata?.name || "Git",
    status,
    refs,
    history,
    identity,
    setIdentity,
    remote,
    remoteName,
    setRemoteName,
    error,
    setError,
    busy,
    setBusy,
    loaded,
    load,
  };
}

export type GitRepository = ReturnType<typeof useGitRepository>;
