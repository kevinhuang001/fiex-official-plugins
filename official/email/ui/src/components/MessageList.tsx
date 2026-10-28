import { t } from "../../../../../support/ui/i18n";
import { Inbox, LoaderCircle } from "lucide-react";
import { initial, senderName, shortDate } from "../format";
import type { Header } from "../types";

export function MessageList({
  messages,
  selected,
  loading,
  query,
  onSelect,
}: {
  messages: Header[];
  selected: number | null;
  loading: boolean;
  query: string;
  onSelect: (message: Header) => void;
}) {
  return (
    <section className="mail-list" aria-label={t("邮件列表")}>
      <header className="mail-list-head">
        <div>
          <h1>{t("收件箱")}</h1>
          <p>
            {query
              ? t("搜索“{0}”", query)
              : t("{0} 封最近的邮件", messages.length)}
          </p>
        </div>
        {loading && <LoaderCircle size={15} className="is-spinning" />}
      </header>
      {loading && !messages.length ? (
        <div className="mail-skeletons" aria-label={t("正在加载")}>
          {Array.from({ length: 5 }).map((_, index) => (
            <span key={index} style={{ animationDelay: `${index * 60}ms` }} />
          ))}
        </div>
      ) : messages.length ? (
        <div className="mail-rows" role="list">
          {messages.map((message) => (
            <button
              key={message.seq}
              type="button"
              role="listitem"
              className={selected === message.seq ? "is-selected" : undefined}
              onClick={() => onSelect(message)}
            >
              <span className="mail-row-avatar">
                {initial(message.from, "")}
              </span>
              <span className="mail-row-copy">
                <strong>{senderName(message.from)}</strong>
                <span>{message.subject || t("（无主题）")}</span>
              </span>
              <time>{shortDate(message.date)}</time>
            </button>
          ))}
        </div>
      ) : (
        <div className="mail-empty">
          <Inbox size={22} />
          <p>{query ? t("没有匹配的邮件") : t("收件箱是空的")}</p>
        </div>
      )}
    </section>
  );
}
