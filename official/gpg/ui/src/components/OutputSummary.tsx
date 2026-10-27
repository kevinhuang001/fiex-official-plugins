import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { outputName } from "../paths";
import type { Action } from "../types";

export function OutputSummary({
  files,
  action,
  armor,
  directory,
}: {
  files: ResourceContext[];
  action: Action;
  armor: boolean;
  directory: ResourceContext | undefined;
}) {
  return (
    <div className="gpg-out">
      <span>输出</span>
      <code>{outputName(files[0] || "文件", action, armor)}</code>
      <small>{directory ? `保存到 ${directory.metadata?.name || "所选目录"}` : "选择保存位置"}</small>
    </div>
  );
}
