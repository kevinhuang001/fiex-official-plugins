import { t } from "../../../../../support/ui/i18n";
import { LoaderCircle, Send, X } from "lucide-react";
import { useState } from "react";

export function Composer({
  busy,
  onClose,
  onSend,
}: {
  busy: boolean;
  onClose: () => void;
  onSend: (to: string, subject: string, body: string) => Promise<void>;
}) {
  const [to, setTo] = useState("");
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");
  const submit = async () => {
    if (!to.trim() || !body.trim()) return;
    await onSend(to, subject, body);
  };
  return (
    <div className="mail-composer-backdrop">
      <div className="mail-composer">
        <header>
          <span>{t("新邮件")}</span>
          <button type="button" onClick={onClose} aria-label={t("关闭")}>
            <X size={16} />
          </button>
        </header>
        <label>
          <span>{t("收件人")}</span>
          <input
            type="email"
            value={to}
            onChange={(event) => setTo(event.target.value)}
            placeholder="name@example.com"
            required
          />
        </label>
        <label>
          <span>{t("主题")}</span>
          <input
            value={subject}
            onChange={(event) => setSubject(event.target.value)}
            placeholder={t("邮件主题")}
          />
        </label>
        <textarea
          value={body}
          onChange={(event) => setBody(event.target.value)}
          placeholder={t("写点什么…")}
          required
        />
        <footer>
          <button type="button" className="mail-secondary" onClick={onClose}>
            {t("取消")}{" "}
          </button>
          <button
            type="button"
            className="mail-primary"
            disabled={busy || !to.trim() || !body.trim()}
            onClick={() => void submit()}
          >
            {busy ? (
              <LoaderCircle size={14} className="is-spinning" />
            ) : (
              <Send size={14} />
            )}
            {busy ? t("发送中…") : t("发送")}
          </button>
        </footer>
      </div>
    </div>
  );
}
