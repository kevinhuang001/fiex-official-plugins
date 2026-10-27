import type { ResourceRef } from "../../../../sdk/ui/src/index";
/** View models shared by the git panel hooks and components. */
export interface StatusEntry {
  path: string;
  code: string;
}
export interface Status {
  root: ResourceRef;
  branch: string;
  entries: Record<string, string>;
}
export interface Refs {
  head: string;
  current: string;
  branches: string[];
  tags: string[];
}
export interface Commit {
  id: string;
  shortId: string;
  subject: string;
  author: string;
  timestamp: number;
  refs: string;
}
export interface Identity {
  name: string;
  email: string;
}
export interface RemoteStatus {
  remotes: { name: string; url: string }[];
  branch: string;
  upstream: string;
  ahead: number;
  behind: number;
}
export interface Context {
  directory?: import("../../../../sdk/ui/src/index").ResourceContext;
  targets?: import("../../../../sdk/ui/src/index").ResourceContext[];
}

/** A status entry with its parsed change state, ready to render. */
export interface ChangeEntry extends StatusEntry {
  letter: string;
  label: string;
  staged: boolean;
  unstaged: boolean;
}

/** A commit plus whether it is the one HEAD points at. */
export interface HistoryEntry extends Commit {
  current: boolean;
}

/** The remote dialog's add/edit form payload. */
export interface RemoteEdit {
  mode: "new" | "edit";
  name: string;
  url: string;
}
