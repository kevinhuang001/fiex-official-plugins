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
          aria-label="选择 commit"
          disabled={busy}
          onClick={onOpenPicker}
        >
          <GitBranch size={13} />
          <span>
            {refs?.current || status.branch} · {refs?.head || "尚无 commit"}
          </span>
          <ChevronDown size={12} />
        </button>
        <button
          className="icon-button"
          title="远程仓库"
          aria-label="远程仓库"
          onClick={onOpenRemote}
        >
          <Cloud size={15} />
        </button>
        <button
          className="icon-button"
          title="刷新"
          aria-label="刷新源代码管理"
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
          {stagedCount ? `提交修改 · ${stagedCount}` : "提交修改"}
        </button>
      </div>
    </>
  );
}
