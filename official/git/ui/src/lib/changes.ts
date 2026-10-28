import { t } from "../../../../../support/ui/i18n";
import type { ChangeEntry, Status } from "../types";

/** Last path segment, used for the short file label. */
export const baseName = (path: string) => path.split(/[/\\]/).pop() || path;

/** Parses a `git status` two-letter code into display state. */
const changeOf = (code: string): Omit<ChangeEntry, "path" | "code"> | null => {
  if (code === "??")
    return { letter: "U", label: t("未跟踪"), staged: false, unstaged: true };
  const index = code[0] ?? " ";
  const work = code[1] ?? " ";
  const staged = index !== " " && index !== "?";
  const unstaged = work !== " " && work !== "?";
  const names: Record<string, string> = {
    M: t("修改"),
    A: t("新增"),
    D: t("删除"),
    R: t("重命名"),
    C: t("复制"),
    T: t("类型变更"),
    U: t("冲突"),
  };
  const letter = unstaged ? work : index;
  if (letter === " ") return null;
  const label = names[letter] ?? letter;
  return {
    letter,
    label:
      staged && unstaged
        ? t("已暂存并{0}", label)
        : staged
          ? t("已暂存{0}", label)
          : t("未暂存{0}", label),
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
