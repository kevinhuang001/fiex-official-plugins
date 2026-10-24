import { useMemo } from "react";
import type { Commit, HistoryEntry } from "../types";

/** Annotates history rows with whether they are the commit HEAD points at. */
export function useGitHistory(
  history: Commit[],
  head: string | undefined,
): HistoryEntry[] {
  return useMemo(
    () =>
      history.map((item) => ({
        ...item,
        current:
          item.id === head ||
          item.shortId === head ||
          Boolean(head?.startsWith(item.shortId)),
      })),
    [history, head],
  );
}
