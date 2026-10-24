import { outputName } from "../paths";
import type { Action } from "../types";

export function OutputSummary({
  files,
  action,
  armor,
  directory,
}: {
  files: string[];
  action: Action;
  armor: boolean;
  directory: string;
}) {
  return (
    <div className="gpg-out">
      <span>输出</span>
      <code>{outputName(files[0] || "文件", action, armor)}</code>
      <small>{directory ? `保存到 ${directory}` : "保存到原目录"}</small>
    </div>
  );
}
