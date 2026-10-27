import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { FileList } from "./FileList";
import { KeyPicker } from "./KeyPicker";
import { OutputSummary } from "./OutputSummary";
import { Toggle } from "./Toggle";
import type { Key } from "../types";

export function SignForm({
  files,
  directory,
  keys,
  totalKeys,
  fingerprint,
  onFingerprint,
  query,
  onQuery,
  busy,
  armor,
  onArmor,
}: {
  files: ResourceContext[];
  directory: ResourceContext | undefined;
  keys: Key[];
  totalKeys: number;
  fingerprint: string;
  onFingerprint: (value: string) => void;
  query: string;
  onQuery: (value: string) => void;
  busy: boolean;
  armor: boolean;
  onArmor: (value: boolean) => void;
}) {
  return (
    <>
      <FileList files={files} />
      <OutputSummary
        files={files}
        action="sign"
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
        kind="secret"
      />
      <div className="gpg-options">
        <Toggle
          label="ASCII 装甲签名"
          checked={armor}
          onChange={onArmor}
          disabled={busy}
        />
      </div>
    </>
  );
}
