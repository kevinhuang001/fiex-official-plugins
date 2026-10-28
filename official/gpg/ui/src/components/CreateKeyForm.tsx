import { t } from "../../../../../support/ui/i18n";
import { useState } from "react";
import { KeyRound } from "lucide-react";
import type { CreateKeyInput } from "../types";

/** New key pair form. The passphrase is sent to Core over stdin and never
 *  rendered elsewhere. */
export function CreateKeyForm({
  onCreate,
  onDone,
  onCancel,
  busy,
}: {
  onCreate: (input: CreateKeyInput) => Promise<void>;
  onDone: () => void;
  onCancel: () => void;
  busy: boolean;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [expiry, setExpiry] = useState("0");
  const [passphrase, setPassphrase] = useState("");
  const [error, setError] = useState("");

  const submit = async () => {
    if (!name.trim() || !email.trim()) {
      setError(t("请填写名称和邮箱"));
      return;
    }
    setError("");
    try {
      await onCreate({
        name: name.trim(),
        email: email.trim(),
        expiry,
        passphrase,
      });
      onDone();
    } catch (cause) {
      setError(String(cause));
    }
  };

  return (
    <div className="gpg-create">
      <header className="gpg-create-head">
        <span className="gpg-create-mark" aria-hidden>
          <KeyRound size={18} />
        </span>
        <div>
          <h2>{t("新建密钥")}</h2>
          <p>{t("使用本机 GnuPG 生成新的密钥对。")}</p>
        </div>
      </header>
      <div className="gpg-create-grid">
        <label className="gpg-field">
          <span>{t("名称")}</span>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t("例如 Ada Lovelace")}
            autoFocus
            disabled={busy}
          />
        </label>
        <label className="gpg-field">
          <span>{t("邮箱")}</span>
          <input
            type="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            placeholder="ada@example.org"
            disabled={busy}
          />
        </label>
        <label className="gpg-field">
          <span>{t("有效期")}</span>
          <select
            value={expiry}
            onChange={(event) => setExpiry(event.target.value)}
            disabled={busy}
          >
            <option value="0">{t("永久")}</option>
            <option value="1y">{t("1 年")}</option>
            <option value="2y">{t("2 年")}</option>
            <option value="5y">{t("5 年")}</option>
          </select>
        </label>
        <label className="gpg-field">
          <span>{t("口令（可选）")}</span>
          <input
            type="password"
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
            placeholder={t("留空则不设置口令")}
            disabled={busy}
          />
        </label>
      </div>
      <p className="gpg-hint">
        {t(
          "口令只通过 stdin 交给本机 gpg，不会写入插件存储。留空表示密钥不设口令保护。",
        )}{" "}
      </p>
      {error && (
        <p className="gpg-create-error" role="alert">
          {error}
        </p>
      )}
      <footer className="gpg-create-actions">
        <button type="button" onClick={onCancel} disabled={busy}>
          {t("取消")}{" "}
        </button>
        <button
          type="button"
          className="gpg-primary"
          disabled={busy}
          onClick={() => void submit()}
        >
          {busy ? t("正在创建…") : t("创建密钥")}
        </button>
      </footer>
    </div>
  );
}
