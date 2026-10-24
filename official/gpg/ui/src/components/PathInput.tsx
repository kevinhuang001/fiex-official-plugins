export function PathInput({
  value,
  onPick,
  disabled,
}: {
  value: string;
  onPick: () => void;
  disabled: boolean;
}) {
  return (
    <div className="gpg-path">
      <input value={value} readOnly placeholder="尚未选择文件" />
      <button type="button" onClick={onPick} disabled={disabled}>
        选择
      </button>
    </div>
  );
}
