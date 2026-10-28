import { useSyncExternalStore } from "react";
import { i18n } from "../../../../support/ui/i18n";
import { t } from "../../../../support/ui/i18n";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Mail, RefreshCw, Search } from "lucide-react";
import * as api from "./api";
import { Composer } from "./components/Composer";
import { MessageList } from "./components/MessageList";
import { ReadingPane } from "./components/ReadingPane";
import { Sidebar } from "./components/Sidebar";
import type { Account, Body, Context, Header, Status } from "./types";

export function App() {
  useSyncExternalStore(i18n.subscribe, i18n.getLocale, i18n.getLocale);
  const [account, setAccount] = useState<Account | null>(null);
  const [messages, setMessages] = useState<Header[]>([]);
  const [selected, setSelected] = useState<Header | null>(null);
  const [message, setMessage] = useState<Body | null>(null);
  const [loading, setLoading] = useState(false);
  const [reading, setReading] = useState(false);
  const [composing, setComposing] = useState(false);
  const [sending, setSending] = useState(false);
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  const load = useCallback(async (target: Account) => {
    setLoading(true);
    setStatus({ kind: "idle" });
    try {
      const next = await api.list(target);
      setMessages(next);
      setStatus({
        kind: "ok",
        text: next.length
          ? t("已同步 {0} 封邮件", next.length)
          : t("收件箱是空的"),
      });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void api.client
      .invoke<Context>("context.get")
      .then((context) => {
        if (cancelled) return;
        const state = context.state;
        if (!state?.email || !state.connectionId) {
          setStatus({ kind: "error", text: t("请从“连接”中打开邮箱") });
          return;
        }
        const target: Account = {
          connectionId: state.connectionId,
          email: state.email,
          imapHost: state.imapHost || "imap.qq.com",
          smtpHost: state.smtpHost || "smtp.qq.com",
        };
        setAccount(target);
        void load(target);
      })
      .catch((error) => {
        if (!cancelled) setStatus({ kind: "error", text: String(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [load]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return messages;
    return messages.filter((item) =>
      `${item.from} ${item.subject}`.toLowerCase().includes(needle),
    );
  }, [messages, query]);

  const open = async (item: Header) => {
    if (!account) return;
    setSelected(item);
    setMessage(null);
    setReading(true);
    try {
      setMessage(await api.read(account, item.seq));
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
    } finally {
      setReading(false);
    }
  };

  const send = async (to: string, subject: string, body: string) => {
    if (!account) return;
    setSending(true);
    try {
      await api.send(account, to.trim(), subject.trim(), body);
      setComposing(false);
      setStatus({ kind: "ok", text: t("邮件已发送") });
    } catch (error) {
      setStatus({ kind: "error", text: String(error) });
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="mail-app">
      {account ? (
        <Sidebar
          account={account}
          status={status}
          busy={loading}
          onCompose={() => setComposing(true)}
          onRefresh={() => void load(account)}
        />
      ) : (
        <aside className="mail-sidebar mail-sidebar-empty">
          <div className="mail-brand">
            <span className="mail-brand-mark">
              <Mail size={17} />
            </span>
            <span>Fiex Mail</span>
          </div>
        </aside>
      )}
      <main className="mail-main">
        <header className="mail-topbar">
          <label className="mail-search">
            <Search size={15} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("搜索邮件")}
              disabled={!account}
            />
          </label>
          <button
            type="button"
            className="mail-icon-button"
            onClick={() => account && void load(account)}
            disabled={!account || loading}
            aria-label={t("刷新")}
          >
            <RefreshCw
              size={15}
              className={loading ? "is-spinning" : undefined}
            />
          </button>
          <span className="mail-topbar-account">
            {account?.email || t("未连接")}
          </span>
        </header>
        {status.kind === "error" && (
          <div className="mail-error" role="alert">
            {status.text}
          </div>
        )}
        <div className="mail-content">
          <MessageList
            messages={filtered}
            selected={selected?.seq ?? null}
            loading={loading}
            query={query}
            onSelect={(item) => void open(item)}
          />
          <ReadingPane
            message={message}
            loading={reading}
            onBack={() => {
              setSelected(null);
              setMessage(null);
            }}
          />
        </div>
      </main>
      {composing && account && (
        <Composer
          busy={sending}
          onClose={() => setComposing(false)}
          onSend={send}
        />
      )}
    </div>
  );
}
