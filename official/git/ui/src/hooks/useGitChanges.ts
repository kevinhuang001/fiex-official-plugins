import { useMemo } from "react";
import { collectChanges } from "../lib/changes";
import type { Status } from "../types";

/** Derives the sorted change list and its staged/unstaged partitions. */
export function useGitChanges(status: Status | null) {
  const changes = collectChanges(status);
  const staged = useMemo(
    () => changes.filter((item) => item.staged),
    [changes],
  );
  const unstaged = useMemo(
    () => changes.filter((item) => item.unstaged),
    [changes],
  );
  return { changes, staged, unstaged };
}
