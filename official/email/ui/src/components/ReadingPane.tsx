import { t } from "../../../../../support/ui/i18n";
import { ArrowLeft, LoaderCircle, MailOpen } from "lucide-react";
import { fullDate, initial, senderAddress, senderName } from "../format";
import type { Body } from "../types";

export function ReadingPane({
  message,
  loading,
  onBack,
}: {
  message: Body | null;
  loading: boolean;
  onBack: () => void;
}) {
  if (loading) {
    return (
      <section className="mail-reader is-loading">
        <LoaderCircle size={18} className="is-spinning" />
      </section>
    );
  }
  if (!message) {
    return (
      <section className="mail-reader mail-reader-empty">
        <span className="mail-reader-mark">
          <MailOpen size={26} />
        </span>
        <h2>{t("选择一封邮件")}</h2>
        <p>{t("邮件内容会显示在这里。凭据由 Fiex 按设置保存。")}</p>
      </section>
    );
  }
  return (
    <section className="mail-reader">
      <header>
        <button
          type="button"
          className="mail-back"
          onClick={onBack}
          aria-label={t("返回列表")}
        >
          <ArrowLeft size={15} />
        </button>
        <h2>{message.subject || t("（无主题）")}</h2>
        <div className="mail-reader-meta">
          <span className="mail-row-avatar">{initial(message.from, "")}</span>
          <div>
            <strong>{senderName(message.from)}</strong>
            <small>{senderAddress(message.from)}</small>
          </div>
          <time>{fullDate(message.date)}</time>
        </div>
      </header>
      <pre className="mail-body">
        {message.body || t("（此邮件没有纯文本内容）")}
      </pre>
    </section>
  );
}
