import { RefreshCw, Search, Upload } from "lucide-react";
import type { Key } from "../types";

export function KeysView({
  keys,
  total,
  query,
  onQuery,
  onExport,
  onImport,
  onRefresh,
  busy,
}: {
  keys: Key[];
  total: number;
  query: string;
  onQuery: (value: string) => void;
  onExport: (key: Key) => void;
  onImport: () => void;
  onRefresh: () => void;
  busy: boolean;
}) {
  return (
    <>
      <div className="gpg-keys-head">
        <span>{total} 个公钥</span>
        <div className="gpg-actions">
          <button type="button" onClick={onImport} disabled={busy}>
            <Upload size={13} />
            导入
          </button>
          <button type="button" onClick={onRefresh} disabled={busy}>
            <RefreshCw size={13} />
            刷新
          </button>
        </div>
      </div>
      {total > 4 && (
        <div className="gpg-search wide">
          <Search size={12} />
          <input
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder="搜索名称、邮箱或指纹"
            disabled={busy}
          />
        </div>
      )}
      <ul className="gpg-key-list full">
        {keys.map((key) => (
          <li key={key.fingerprint} className="gpg-key-row">
            <div className="gpg-key-text">
              <strong>{key.name || key.email || key.keyId}</strong>
              <small>{key.fingerprint}</small>
            </div>
            <button type="button" onClick={() => onExport(key)} disabled={busy}>
              导出
            </button>
          </li>
        ))}
        {keys.length === 0 && <li className="gpg-empty">没有匹配的公钥</li>}
      </ul>
    </>
  );
}
