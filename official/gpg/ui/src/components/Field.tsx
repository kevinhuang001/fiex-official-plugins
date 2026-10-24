import type { ReactNode } from "react";

export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="gpg-field">
      <span>{label}</span>
      {children}
    </label>
  );
}
