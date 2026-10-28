import { t } from "../../../../../support/ui/i18n";
import type { ActionMeta } from "../types";

export function Header({
  meta,
  showCount,
  count,
}: {
  meta: ActionMeta;
  showCount: boolean;
  count: number;
}) {
  return (
    <header className="gpg-head">
      <div className="gpg-title">
        <span className="gpg-mark" aria-hidden>
          <meta.icon size={15} />
        </span>
        <div>
          <h1>{meta.label}</h1>
          <p>{meta.heading}</p>
        </div>
      </div>
      {showCount && <span className="gpg-count">{t("{0} 个文件", count)}</span>}
    </header>
  );
}
