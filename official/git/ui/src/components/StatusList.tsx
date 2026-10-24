import { FileCode2, Minus, Plus } from "lucide-react";
import { baseName } from "../lib/changes";
import type { ChangeEntry } from "../types";
import { CleanState } from "./PanelStates";

export function StatusList({
  root,
  changes,
  staged,
  unstaged,
  busy,
  onToggle,
}: {
  root: string;
  changes: ChangeEntry[];
  staged: ChangeEntry[];
  unstaged: ChangeEntry[];
  busy: boolean;
  onToggle: (items: ChangeEntry[], staged: boolean) => void;
}) {
  return (
    <div className="git-panel-scroll">
      {!changes.length && <CleanState />}
      {[
        { title: "已暂存", items: staged, staged: true },
        { title: "更改", items: unstaged, staged: false },
      ]
        .filter((section) => section.items.length)
        .map((section) => (
          <section key={section.title}>
            <h2>
              <span>{section.title}</span>
              <small>{section.items.length}</small>
              <button
                className="git-section-action"
                disabled={busy}
                onClick={() => onToggle(section.items, section.staged)}
              >
                {section.staged ? <Minus size={12} /> : <Plus size={12} />}
                {section.staged ? "全部取消暂存" : "暂存全部"}
              </button>
            </h2>
            {section.items.map((item) => (
              <div className="git-change-row" key={item.path}>
                <button className="git-change-open" title={item.path}>
                  <FileCode2 size={15} />
                  <span className="git-change-name">
                    <span
                      className={`git-file git-file-${item.letter.toLowerCase()}`}
                    >
                      {baseName(item.path)}
                    </span>
                    <small>{item.path.slice(root.length + 1)}</small>
                  </span>
                </button>
                <span
                  className={`git-change-badge git-file-${item.letter.toLowerCase()}`}
                  title={item.label}
                >
                  {item.code === "??"
                    ? "U"
                    : section.staged
                      ? item.code[0]
                      : item.code[1]}
                </span>
                <button
                  className="git-file-action"
                  aria-label={`${section.staged ? "取消暂存" : "暂存"} ${baseName(item.path)}`}
                  title={section.staged ? "取消暂存" : "暂存"}
                  disabled={busy}
                  onClick={() => onToggle([item], section.staged)}
                >
                  {section.staged ? (
                    <Minus size={14} />
                  ) : (
                    <Plus size={14} />
                  )}
                </button>
              </div>
            ))}
          </section>
        ))}
    </div>
  );
}
