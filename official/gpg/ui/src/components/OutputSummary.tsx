import { t } from "../../../../../support/ui/i18n";
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
      <span>{t("输出")}</span>
      <code>{outputName(files[0] || t("文件"), action, armor)}</code>
      <small>
        {directory
          ? t("保存到 {0}", directory.metadata?.name || t("所选目录"))
          : t("选择保存位置")}
      </small>
    </div>
  );
}
