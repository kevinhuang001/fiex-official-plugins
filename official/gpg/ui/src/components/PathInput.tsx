import type { ResourceContext } from "../../../../../sdk/ui/src/index";
export function PathInput({
  value,
  onPick,
  disabled,
}: {
  value: ResourceContext | undefined;
  onPick: () => void;
  disabled: boolean;
}) {
  return (
    <div className="gpg-path">
      <input value={value?.metadata?.name || ""} readOnly placeholder="尚未选择文件" />
      <button type="button" onClick={onPick} disabled={disabled}>
        选择
      </button>
    </div>
  );
}
