import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { Field } from "./Field";
import { PathInput } from "./PathInput";

export function VerifyForm({
  signature,
  data,
  onPickSignature,
  onPickData,
  busy,
}: {
  signature: ResourceContext | undefined;
  data: ResourceContext | undefined;
  onPickSignature: () => void;
  onPickData: () => void;
  busy: boolean;
}) {
  return (
    <>
      <Field label="签名文件">
        <PathInput value={signature} onPick={onPickSignature} disabled={busy} />
      </Field>
      <Field label="被签名的文件">
        <PathInput value={data} onPick={onPickData} disabled={busy} />
      </Field>
      <p className="gpg-hint">
        支持分离签名（.sig / .asc），签名者由本机公钥环验证。
      </p>
    </>
  );
}
