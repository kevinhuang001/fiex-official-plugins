import { useEffect, useRef, useState } from "react";
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
  const [up, setUp] = useState(false);
  const root = useRef<HTMLDivElement>(null);
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
        aria-label="同步分支"
        onClick={() => {
          const rect = root.current?.getBoundingClientRect();
          setUp(
            Boolean(
              rect &&
              rect.bottom + Math.min(180, options.length * 32 + 8) >
                window.innerHeight - 8,
            ),
          );
          setOpen(!open);
        }}
      >
        {value || "选择分支"}
        <ChevronDown size={13} className={open ? "open" : ""} />
      </button>
      {open && (
        <div
          className={`branch-select-menu${up ? " open-up" : ""}`}
          role="listbox"
          aria-label="同步分支"
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
