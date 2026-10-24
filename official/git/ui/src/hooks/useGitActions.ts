import { useCallback } from "react";
import type { FiexUiClient } from "../../../../../sdk/ui/src/index";
import type { StatusEntry } from "../types";
import { invokeGit } from "./useGitClient";
import type { GitRepository } from "./useGitRepository";

/**
 * Repository mutations. Every action funnels through the same busy/error pair
 * and reloads the snapshot afterwards, exactly like the panel always has.
 */
export function useGitActions(client: FiexUiClient, repository: GitRepository) {
  const { root, identity, load, setBusy, setError } = repository;

  const run = useCallback(
    async (action: () => Promise<unknown>): Promise<void> => {
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
    },
    [load, setBusy, setError],
  );

  const runTask = useCallback(
    async (
      title: string,
      detail: string,
      action: (taskId: string) => Promise<string | void>,
    ): Promise<void> => {
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
    },
    [client, load, setBusy, setError],
  );

  const toggle = useCallback(
    (items: StatusEntry[], staged: boolean) => {
      const paths = items.map((item) => item.path);
      if (!paths.length) return;
      void run(() =>
        invokeGit(client, staged ? "git.unstage" : "git.stage", {
          root,
          paths,
          all: false,
        }),
      );
    },
    [client, root, run],
  );

  const commit = useCallback(
    (message: string, onCommitted: () => void) => {
      if (!message.trim()) return;
      void runTask("提交更改", message.split("\n")[0], async (taskId) => {
        await client.tasks.update(taskId, { step: "写入提交" });
        const hash = await invokeGit<string>(client, "git.commit", {
          root,
          message,
          name: identity.name,
          email: identity.email,
        });
        onCommitted();
        return `已提交 ${hash}`;
      });
    },
    [client, identity, root, runTask],
  );

  const init = useCallback(
    () =>
      void runTask("初始化 Git 仓库", root, async (taskId) => {
        await client.tasks.update(taskId, { step: "创建 .git" });
        await invokeGit(client, "git.init", { root });
        return "仓库已创建";
      }),
    [client, root, runTask],
  );

  const openCommit = useCallback(
    (id: string) => {
      void invokeGit<string>(client, "git.browse", { root, reference: id })
        .then((path) => client.invoke("navigation.open", { path }))
        .then(() => client.invoke("panel.close"))
        .catch((cause) => setError(String(cause)));
    },
    [client, root, setError],
  );

  return { run, runTask, toggle, commit, init, openCommit };
}

export type GitActions = ReturnType<typeof useGitActions>;
