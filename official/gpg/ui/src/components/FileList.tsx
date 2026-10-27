import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { FileSignature } from "lucide-react";
import { baseName } from "../paths";

export function FileList({ files }: { files: ResourceContext[] }) {
  if (!files.length) return <p className="gpg-empty">没有选中的文件</p>;
  return (
    <ul className="gpg-files">
      {files.slice(0, 4).map((file) => (
        <li key={JSON.stringify(file.ref)} title={baseName(file)}>
          <FileSignature size={13} />
          <span>{baseName(file)}</span>
        </li>
      ))}
      {files.length > 4 && <li className="gpg-more">+{files.length - 4}</li>}
    </ul>
  );
}
