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
        <h2>选择一封邮件</h2>
        <p>邮件内容会显示在这里。你的凭据只保存在系统钥匙串中。</p>
      </section>
    );
  }
  return (
    <section className="mail-reader">
      <header>
        <button type="button" className="mail-back" onClick={onBack} aria-label="返回列表">
          <ArrowLeft size={15} />
        </button>
        <h2>{message.subject || "（无主题）"}</h2>
        <div className="mail-reader-meta">
          <span className="mail-row-avatar">{initial(message.from, "")}</span>
          <div>
            <strong>{senderName(message.from)}</strong>
            <small>{senderAddress(message.from)}</small>
          </div>
          <time>{fullDate(message.date)}</time>
        </div>
      </header>
      <pre className="mail-body">{message.body || "（此邮件没有纯文本内容）"}</pre>
    </section>
  );
}
