import { useCallback, useEffect, useState } from "react";
import type { FiexUiClient } from "../../../../../sdk/ui/src/index";
import type { RemoteEdit } from "../types";
import { invokeGit } from "./useGitClient";
import type { GitActions } from "./useGitActions";
import type { GitRepository } from "./useGitRepository";

/**
 * Local state of the remote dialog: the add/edit form, the pending-remove
 * confirmation and the branch picked for pull/push. Opening the dialog (or
 * the checked-out branch changing) resets all three.
 */
export function useRemotePanel(
  client: FiexUiClient,
  repository: GitRepository,
  actions: GitActions,
  open: boolean,
) {
  const { root, refs, remote, remoteName, setRemoteName } = repository;
  const { run, runTask } = actions;
  const currentBranch = refs?.current;
  const [editing, setEditing] = useState<RemoteEdit | null>(null);
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);
  const [syncBranch, setSyncBranch] = useState("");

  useEffect(() => {
    if (!open) return;
    setEditing(null);
    setPendingRemove(null);
    setSyncBranch(currentBranch || "");
  }, [open, currentBranch]);

  const save = useCallback(
    (nextName: string, nextUrl: string) => {
      if (!nextName.trim() || !nextUrl.trim()) return;
      void run(async () => {
        await invokeGit(client, "git.remoteSave", {
          root,
          name: nextName.trim(),
          url: nextUrl.trim(),
        });
        setRemoteName(nextName.trim());
        setEditing(null);
      });
    },
    [client, root, run, setRemoteName],
  );

  const remove = useCallback(
    (target: string) =>
      void run(async () => {
        await invokeGit(client, "git.remoteRemove", { root, name: target });
        setPendingRemove(null);
        if (remoteName === target) setRemoteName("origin");
      }),
    [client, remoteName, root, run, setRemoteName],
  );

  const sync = useCallback(
    (push: boolean) =>
      void runTask(
        push ? "推送分支" : "拉取分支",
        `${remoteName}/${syncBranch || remote?.branch || "当前分支"}`,
        async (taskId) => {
          if (!remoteName.trim()) throw new Error("请先配置远程仓库");
          await client.tasks.update(taskId, {
            step: "连接远程仓库",
            progress: 0.25,
          });
          await invokeGit(client, "git.remoteSync", {
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
      ),
    [client, remote, remoteName, root, runTask, syncBranch],
  );

  return {
    editing,
    setEditing,
    pendingRemove,
    setPendingRemove,
    syncBranch,
    setSyncBranch,
    save,
    remove,
    sync,
  };
}
