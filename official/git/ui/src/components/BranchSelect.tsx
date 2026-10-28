import { t } from "../../../../../support/ui/i18n";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

export function BranchSelect({
  value,
  options,
  onChange,
}: {
  value: string;
  options: string[];
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [placement, setPlacement] = useState({ up: false, maxHeight: 180 });
  const root = useRef<HTMLDivElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  // Measure against the nearest scrolling sheet, not the whole iframe: the
  // remote sheet clips its overflow, so an iframe-height check lists the menu
  // downward and cuts it off at the sheet's bottom edge.
  useLayoutEffect(() => {
    if (!open) return;
    const trigger = root.current?.getBoundingClientRect();
    if (!trigger) return;
    let top = 8;
    let bottom = window.innerHeight - 8;
    let ancestor = root.current?.parentElement;
    while (ancestor) {
      const style = getComputedStyle(ancestor);
      if (style.overflowY !== "visible") {
        const rect = ancestor.getBoundingClientRect();
        top = Math.max(top, rect.top + 6);
        bottom = Math.min(bottom, rect.bottom - 6);
        break;
      }
      ancestor = ancestor.parentElement;
    }
    const wanted = Math.min(180, options.length * 32 + 8);
    const below = bottom - trigger.bottom - 4;
    const above = trigger.top - top - 4;
    const up = below < Math.min(wanted, 96) && above > below;
    setPlacement({
      up,
      maxHeight: Math.max(64, Math.min(wanted, up ? above : below)),
    });
  }, [open, options.length]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  return (
    <div className="branch-select" ref={root}>
      <button
        type="button"
        className="branch-select-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={t("同步分支")}
        onClick={() => setOpen(!open)}
      >
        {value || t("选择分支")}
        <ChevronDown size={13} className={open ? "open" : ""} />
      </button>
      {open && (
        <div
          ref={menu}
          className={`branch-select-menu${placement.up ? " open-up" : ""}`}
          role="listbox"
          aria-label={t("同步分支")}
          style={{ maxHeight: placement.maxHeight }}
        >
          {options.map((item) => (
            <button
              type="button"
              role="option"
              aria-selected={item === value}
              className={`branch-select-option ${item === value ? "active" : ""}`}
              key={item}
              onClick={() => {
                onChange(item);
                setOpen(false);
              }}
            >
              {item}
              {item === value && <Check size={12} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
