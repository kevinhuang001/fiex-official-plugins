import type { ChangeEntry, Status } from "../types";

/** Last path segment, used for the short file label. */
export const baseName = (path: string) => path.split(/[/\\]/).pop() || path;

/** Parses a `git status` two-letter code into display state. */
const changeOf = (code: string): Omit<ChangeEntry, "path" | "code"> | null => {
  if (code === "??")
    return { letter: "U", label: "未跟踪", staged: false, unstaged: true };
  const index = code[0] ?? " ";
  const work = code[1] ?? " ";
  const staged = index !== " " && index !== "?";
  const unstaged = work !== " " && work !== "?";
  const names: Record<string, string> = {
    M: "修改",
    A: "新增",
    D: "删除",
    R: "重命名",
    C: "复制",
    T: "类型变更",
    U: "冲突",
  };
  const letter = unstaged ? work : index;
  if (letter === " ") return null;
  const label = names[letter] ?? letter;
  return {
    letter,
    label:
      staged && unstaged
        ? `已暂存并${label}`
        : staged
          ? `已暂存${label}`
          : `未暂存${label}`,
    staged,
    unstaged,
  };
};

/** Sorted, renderable change list for a repository status. */
export const collectChanges = (status: Status | null): ChangeEntry[] =>
  status
    ? Object.entries(status.entries)
        .sort(([a], [b]) => a.localeCompare(b))
        .flatMap(([path, code]) => {
          const state = changeOf(code);
          return state ? [{ path, code, ...state }] : [];
        })
    : [];
