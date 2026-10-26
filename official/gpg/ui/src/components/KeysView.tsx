import { useState } from "react";
import { KeyRound, Plus, RefreshCw, Search, Upload } from "lucide-react";
import type { CreateKeyInput, Key } from "../types";
import { CreateKeyForm } from "./CreateKeyForm";
import { DeleteKeyDialog } from "./DeleteKeyDialog";
import { KeyContextMenu } from "./KeyContextMenu";

function displayName(key: Key) {
  return key.name.replace(/\s*<[^>]*>\s*$/, "") || key.email || key.keyId;
}

function expiry(value: string) {
  if (!value || value === "0") return "永不过期";
  const date = new Date(Number(value) * 1000);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString();
}

function fingerprint(value: string) {
  return value.replace(/(.{4})/g, "$1 ").trim();
}

/** GPG Keychain-style key ring: create, import, search and export or delete
 *  public/private keys from a single table. */
export function KeysView({
  keys,
  total,
  query,
  onQuery,
  onExport,
  onExportSecret,
  onCreate,
  onDelete,
  onImport,
  onRefresh,
  busy,
}: {
  keys: Key[];
  total: number;
  query: string;
  onQuery: (value: string) => void;
  onExport: (key: Key) => void;
  onExportSecret: (key: Key) => void;
  onCreate: (input: CreateKeyInput) => Promise<void>;
  onDelete: (key: Key) => Promise<void>;
  onImport: () => void;
  onRefresh: () => void;
  busy: boolean;
}) {
  const [creating, setCreating] = useState(false);
  const [menu, setMenu] = useState<{ key: Key; x: number; y: number } | null>(
    null,
  );
  const [confirmDelete, setConfirmDelete] = useState<Key | null>(null);

  if (creating) {
    return (
      <CreateKeyForm
        onCreate={onCreate}
        onDone={() => setCreating(false)}
        onCancel={() => setCreating(false)}
        busy={busy}
      />
    );
  }

  return (
    <div className="gpg-keychain">
      <div className="gpg-keychain-tools">
        <label className="gpg-keychain-search">
          <Search size={13} />
          <input
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder="搜索名称、邮箱或指纹"
            disabled={busy}
          />
        </label>
        <div className="gpg-actions">
          <button
            type="button"
            onClick={() => setCreating(true)}
            disabled={busy}
          >
            <Plus size={13} />
            新建
          </button>
          <button type="button" onClick={onImport} disabled={busy}>
            <Upload size={13} />
            导入
          </button>
          <button
            type="button"
            onClick={onRefresh}
            disabled={busy}
            aria-label="刷新密钥"
          >
            <RefreshCw size={13} />
          </button>
        </div>
      </div>
      <div className="gpg-keychain-columns" aria-hidden>
        <span>类型</span>
        <span>名称</span>
        <span>指纹</span>
        <span>密钥 ID</span>
        <span>有效期</span>
      </div>
      <ul className="gpg-keychain-rows" role="listbox" aria-label="密钥">
        {keys.map((key) => (
          <li key={key.fingerprint}>
            <button
              type="button"
              role="option"
              aria-selected={false}
              title="右键导出或删除"
              onContextMenu={(event) => {
                event.preventDefault();
                setMenu({ key, x: event.clientX, y: event.clientY });
              }}
              disabled={busy}
            >
              <span
                className={`gpg-key-type ${key.hasSecret ? "secret" : "public"}`}
              >
                {key.hasSecret ? "pub+sec" : "pub"}
              </span>
              <span className="gpg-keychain-identity">
                <strong>{displayName(key)}</strong>
                <small>{key.email || "未提供邮箱"}</small>
              </span>
              <code
                className="gpg-keychain-fingerprint"
                title={key.fingerprint}
              >
                {fingerprint(key.fingerprint)}
              </code>
              <span className="gpg-keychain-id">{key.keyId}</span>
              <span className="gpg-keychain-expiry">{expiry(key.expires)}</span>
            </button>
          </li>
        ))}
        {keys.length === 0 && (
          <li className="gpg-keychain-empty">
            <KeyRound size={18} />
            <span>{total ? "没有匹配的密钥" : "本机钥匙串为空"}</span>
            {!total && (
              <button type="button" onClick={onImport} disabled={busy}>
                <Upload size={13} />
                导入密钥
              </button>
            )}
          </li>
        )}
      </ul>
      {menu && (
        <KeyContextMenu
          item={menu.key}
          x={menu.x}
          y={menu.y}
          onExport={onExport}
          onExportSecret={onExportSecret}
          onDelete={(key) => setConfirmDelete(key)}
          onClose={() => setMenu(null)}
        />
      )}
      {confirmDelete && (
        <DeleteKeyDialog
          item={confirmDelete}
          busy={busy}
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => {
            const target = confirmDelete;
            setConfirmDelete(null);
            void onDelete(target);
          }}
        />
      )}
    </div>
  );
}
