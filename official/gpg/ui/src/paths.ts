import type { Action } from "./types";

export function baseName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

export function directoryName(path: string) {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index > 0 ? path.slice(0, index) : path;
}

export function join(directory: string, name: string) {
  const separator = directory.includes("\\") ? "\\" : "/";
  return `${directory.replace(/[\\/]+$/, "")}${separator}${name}`;
}

export function withoutSignature(path: string) {
  const lower = path.toLowerCase();
  for (const suffix of [".sig.asc", ".sig", ".asc"]) {
    if (lower.endsWith(suffix)) return path.slice(0, -suffix.length);
  }
  return path;
}

export function isSignature(path: string) {
  const lower = path.toLowerCase();
  return lower.endsWith(".sig") || lower.endsWith(".asc");
}

export function outputName(input: string, action: Action, armor: boolean) {
  const name = baseName(input);
  const lower = name.toLowerCase();
  if (action === "encrypt") {
    return lower.endsWith(".gpg") || lower.endsWith(".asc")
      ? name
      : name + (armor ? ".asc" : ".gpg");
  }
  if (action === "decrypt") {
    for (const suffix of [".gpg", ".pgp", ".asc"]) {
      if (lower.endsWith(suffix)) return name.slice(0, -suffix.length);
    }
    return name + ".decrypted";
  }
  if (action === "sign") return name + (armor ? ".sig.asc" : ".sig");
  return name;
}
