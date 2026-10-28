import { t } from "../../../../../support/ui/i18n";
import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { FileList } from "./FileList";
import { OutputSummary } from "./OutputSummary";

export function DecryptForm({
  files,
  directory,
  armor,
}: {
  files: ResourceContext[];
  directory: ResourceContext | undefined;
  armor: boolean;
}) {
  return (
    <>
      <FileList files={files} />
      <OutputSummary
        files={files}
        action="decrypt"
        armor={armor}
        directory={directory}
      />
      <div className="gpg-options" />
      <p className="gpg-hint">
        {t("私钥口令由系统 pinentry 窗口输入，本面板不会读取。")}{" "}
      </p>
    </>
  );
}
