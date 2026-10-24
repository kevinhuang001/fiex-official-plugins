import type { Status } from "../types";

export function ActionFooter({
  status,
  gpg,
  busy,
  progress,
  fileCount,
  showAction,
  actionLabel,
  canRun,
  onRun,
}: {
  status: Status;
  gpg: string;
  busy: boolean;
  progress: number;
  fileCount: number;
  showAction: boolean;
  actionLabel: string;
  canRun: boolean;
  onRun: () => void;
}) {
  return (
    <footer className="gpg-foot">
      <div className={`gpg-status is-${status.kind}`} role="status">
        {status.kind === "busy" && <span className="gpg-spin" aria-hidden />}
        {status.text || (gpg ? "就绪" : "需要安装 GnuPG 才能使用此插件")}
        {busy && fileCount > 1 && (
          <em>
            {progress}/{fileCount}
          </em>
        )}
      </div>
      {showAction && (
        <button
          className="gpg-primary"
          disabled={!canRun}
          onClick={() => void onRun()}
        >
          {busy ? "处理中…" : actionLabel}
        </button>
      )}
    </footer>
  );
}
