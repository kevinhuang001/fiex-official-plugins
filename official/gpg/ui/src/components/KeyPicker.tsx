import { t } from "../../../../../support/ui/i18n";
import { KeyRound, LoaderCircle, Search } from "lucide-react";
import type { Key } from "../types";

export function KeyPicker({
  keys,
  total,
  value,
  onChange,
  query,
  onQuery,
  busy,
  loading,
  kind = "public",
}: {
  keys: Key[];
  total: number;
  value: string;
  onChange: (value: string) => void;
  query: string;
  onQuery: (value: string) => void;
  busy: boolean;
  loading?: boolean;
  kind?: "public" | "secret";
}) {
  if (loading) {
    return (
      <div className="gpg-empty" aria-busy="true">
        <LoaderCircle className="gpg-spin" size={15} />
        {t("正在加载系统密钥…")}{" "}
      </div>
    );
  }
  if (!total) {
    return (
      <div className="gpg-empty">
        <KeyRound size={15} />
        {kind === "secret"
          ? t("没有可用私钥。请先在 GPG 钥匙串中新建或导入私钥。")
          : t("没有可用公钥。请从工具栏的“GPG 钥匙串”导入密钥。")}
      </div>
    );
  }
  return (
    <div className="gpg-keys">
      <div className="gpg-keys-head">
        <span>{kind === "secret" ? t("签名密钥") : t("收件人")}</span>
        {total > 3 && (
          <div className="gpg-search">
            <Search size={12} />
            <input
              value={query}
              onChange={(event) => onQuery(event.target.value)}
              placeholder={t("搜索")}
              disabled={busy}
            />
          </div>
        )}
      </div>
      <ul className="gpg-key-list">
        {keys.slice(0, 6).map((key) => (
          <li key={key.fingerprint}>
            <button
              type="button"
              aria-pressed={value === key.fingerprint}
              disabled={busy}
              onClick={() => onChange(key.fingerprint)}
            >
              <span className="gpg-radio" aria-hidden />
              <span className="gpg-key-text">
                <strong>{key.name || key.email || key.keyId}</strong>
                <small>{key.fingerprint.slice(-16)}</small>
              </span>
            </button>
          </li>
        ))}
        {keys.length === 0 && (
          <li className="gpg-empty">{t("没有匹配的密钥")}</li>
        )}
      </ul>
    </div>
  );
}
