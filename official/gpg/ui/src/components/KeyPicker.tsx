import { KeyRound, Search } from "lucide-react";
import type { Key } from "../types";

export function KeyPicker({
  keys,
  total,
  value,
  onChange,
  query,
  onQuery,
  busy,
}: {
  keys: Key[];
  total: number;
  value: string;
  onChange: (value: string) => void;
  query: string;
  onQuery: (value: string) => void;
  busy: boolean;
}) {
  if (!total) {
    return (
      <div className="gpg-empty">
        <KeyRound size={15} />
        没有可用公钥。请从工具栏的“公钥管理”导入密钥。
      </div>
    );
  }
  return (
    <div className="gpg-keys">
      <div className="gpg-keys-head">
        <span>收件人</span>
        {total > 3 && (
          <div className="gpg-search">
            <Search size={12} />
            <input
              value={query}
              onChange={(event) => onQuery(event.target.value)}
              placeholder="搜索"
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
        {keys.length === 0 && <li className="gpg-empty">没有匹配的密钥</li>}
      </ul>
    </div>
  );
}
