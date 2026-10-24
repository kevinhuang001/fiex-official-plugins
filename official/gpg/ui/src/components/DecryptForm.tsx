import { FileList } from "./FileList";
import { OutputSummary } from "./OutputSummary";

export function DecryptForm({
  files,
  directory,
  armor,
}: {
  files: string[];
  directory: string;
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
        私钥口令由系统 pinentry 窗口输入，本面板不会读取。
      </p>
    </>
  );
}
