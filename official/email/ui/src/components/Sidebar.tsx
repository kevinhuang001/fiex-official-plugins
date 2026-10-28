import { t } from "../../../../../support/ui/i18n";
import { Inbox, Mail, PenLine, RefreshCw, Send } from "lucide-react";
import type { Account, Status } from "../types";

export function Sidebar({
  account,
  status,
  busy,
  onCompose,
  onRefresh,
}: {
  account: Account;
  status: Status;
  busy: boolean;
  onCompose: () => void;
  onRefresh: () => void;
}) {
  return (
    <aside className="mail-sidebar">
      <div className="mail-brand">
        <span className="mail-brand-mark">
          <Mail size={17} />
        </span>
        <span>Fiex Mail</span>
      </div>
      <button className="mail-compose" type="button" onClick={onCompose}>
        <PenLine size={16} />
        {t("写邮件")}{" "}
      </button>
      <nav className="mail-nav" aria-label={t("邮箱")}>
        <button type="button" className="is-active">
          <Inbox size={16} />
          {t("收件箱")} <em>INBOX</em>
        </button>
        <button type="button" disabled>
          <Send size={16} />
          {t("已发送")}{" "}
        </button>
      </nav>
      <div className="mail-account">
        <span className="mail-avatar">
          {account.email.charAt(0).toUpperCase()}
        </span>
        <span>
          <strong>{account.email.split("@")[0]}</strong>
          <small>{account.email}</small>
        </span>
        <button
          type="button"
          className="mail-icon-button"
          onClick={onRefresh}
          disabled={busy}
          aria-label={t("刷新收件箱")}
          title={t("刷新收件箱")}
        >
          <RefreshCw size={14} className={busy ? "is-spinning" : undefined} />
        </button>
      </div>
      {status.text && (
        <p className={`mail-status is-${status.kind}`} role="status">
          {status.text}
        </p>
      )}
    </aside>
  );
}
