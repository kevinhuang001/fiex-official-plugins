import { t } from "../../../../../support/ui/i18n";
import { CheckCircle2, FolderGit2, LoaderCircle } from "lucide-react";

/** First paint while the host hands over the panel context. */
export function LoadingState() {
  return (
    <div className="git-clean">
      <LoaderCircle size={18} className="spin" />
    </div>
  );
}

/** Shown inside the status list when nothing changed. */
export function CleanState() {
  return (
    <div className="git-clean">
      <CheckCircle2 size={28} />
      <strong>{t("工作区干净")}</strong>
    </div>
  );
}

/** The host opened the panel without a directory. */
export function EmptyFolderState() {
  return (
    <div className="git-welcome">
      <span className="git-welcome-icon">
        <FolderGit2 size={30} />
      </span>
      <p>{t("当前没有可用的文件夹。")}</p>
    </div>
  );
}

/** The directory exists but is not a repository yet. */
export function NotRepositoryState({
  busy,
  onInit,
}: {
  busy: boolean;
  onInit: () => void;
}) {
  return (
    <div className="git-welcome">
      <span className="git-welcome-icon">
        <FolderGit2 size={30} />
      </span>
      <h2>{t("此文件夹还不是 Git 仓库")}</h2>
      <button className="primary" disabled={busy} onClick={onInit}>
        {t("初始化仓库")}{" "}
      </button>
    </div>
  );
}

export function PanelError({ message }: { message: string }) {
  return (
    <div className="git-panel-error" role="alert">
      {message}
    </div>
  );
}

export function FormError({ message }: { message: string }) {
  return (
    <p className="git-panel-error" role="alert">
      {message}
    </p>
  );
}

export function BusyIndicator() {
  return (
    <div className="git-progress" role="status">
      <LoaderCircle size={13} className="spin" /> {t("处理中…")}{" "}
    </div>
  );
}
