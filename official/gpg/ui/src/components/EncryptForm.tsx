import { t } from "../../../../../support/ui/i18n";
import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { FileList } from "./FileList";
import { KeyPicker } from "./KeyPicker";
import { OutputSummary } from "./OutputSummary";
import { Toggle } from "./Toggle";
import type { Key } from "../types";

export function EncryptForm({
  files,
  directory,
  keys,
  totalKeys,
  fingerprint,
  onFingerprint,
  selectedKey,
  query,
  onQuery,
  busy,
  loading,
  armor,
  onArmor,
  trust,
  onTrust,
}: {
  files: ResourceContext[];
  directory: ResourceContext | undefined;
  keys: Key[];
  totalKeys: number;
  fingerprint: string;
  onFingerprint: (value: string) => void;
  selectedKey: Key | undefined;
  query: string;
  onQuery: (value: string) => void;
  busy: boolean;
  loading: boolean;
  armor: boolean;
  onArmor: (value: boolean) => void;
  trust: boolean;
  onTrust: (value: boolean) => void;
}) {
  return (
    <>
      <FileList files={files} />
      <OutputSummary
        files={files}
        action="encrypt"
        armor={armor}
        directory={directory}
      />
      <KeyPicker
        keys={keys}
        total={totalKeys}
        value={fingerprint}
        onChange={onFingerprint}
        query={query}
        onQuery={onQuery}
        busy={busy}
        loading={loading}
      />
      <div className="gpg-options">
        <Toggle
          label={t("ASCII 装甲输出")}
          checked={armor}
          onChange={onArmor}
          disabled={busy}
        />
        <Toggle
          label={t("允许未验证的收件人密钥")}
          checked={trust}
          onChange={onTrust}
          disabled={busy}
        />
      </div>
      {selectedKey && (
        <p className="gpg-hint">
          {t("收件人指纹")} {selectedKey.fingerprint}
        </p>
      )}
    </>
  );
}
