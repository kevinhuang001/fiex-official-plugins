import { t } from "../../../../../support/ui/i18n";
import {
  ChevronDown,
  Cloud,
  FolderGit2,
  GitBranch,
  GitCommitHorizontal,
  RefreshCw,
} from "lucide-react";
import type { Refs, Status } from "../types";

export function RepositoryHeader({
  name,
  status,
  refs,
  busy,
  stagedCount,
  onOpenCommit,
  onOpenPicker,
  onOpenRemote,
  onRefresh,
}: {
  name: string;
  status: Status;
  refs: Refs | null;
  busy: boolean;
  stagedCount: number;
  onOpenCommit: () => void;
  onOpenPicker: () => void;
  onOpenRemote: () => void;
  onRefresh: () => void;
}) {
  return (
    <>
      <div className="git-repository">
        <span className="git-repo-icon">
          <FolderGit2 size={20} />
        </span>
        <span className="git-repo-name">
          <strong>{name}</strong>
        </span>
        <button
          className="git-branch-trigger"
          aria-label={t("选择 commit")}
          disabled={busy}
          onClick={onOpenPicker}
        >
          <GitBranch size={13} />
          <span>
            {refs?.current || status.branch} · {refs?.head || t("尚无 commit")}
          </span>
          <ChevronDown size={12} />
        </button>
        <button
          className="icon-button"
          title={t("远程仓库")}
          aria-label={t("远程仓库")}
          onClick={onOpenRemote}
        >
          <Cloud size={15} />
        </button>
        <button
          className="icon-button"
          title={t("刷新")}
          aria-label={t("刷新源代码管理")}
          disabled={busy}
          onClick={onRefresh}
        >
          <RefreshCw size={14} className={busy ? "spin" : ""} />
        </button>
      </div>

      <div className="git-commit-action">
        <button
          className="primary"
          disabled={busy || !stagedCount}
          onClick={onOpenCommit}
        >
          <GitCommitHorizontal size={15} />
          {stagedCount ? t("提交修改 · {0}", stagedCount) : t("提交修改")}
        </button>
      </div>
    </>
  );
}
