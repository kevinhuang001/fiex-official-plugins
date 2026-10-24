import { FileSignature, KeyRound, Lock, LockOpen, ShieldCheck } from "lucide-react";

export type Action = "encrypt" | "decrypt" | "sign" | "verify" | "keys";

export type Key = {
  fingerprint: string;
  keyId: string;
  name: string;
  email: string;
  expires: string;
};

export type Status = { kind: "idle" | "busy" | "ok" | "error"; text: string };

export type ActionMeta = {
  label: string;
  heading: string;
  icon: typeof Lock;
  width: number;
};

export const DEFAULT_ACTION: Action = "encrypt";

export const ACTIONS: Record<Action, ActionMeta> = {
  encrypt: {
    label: "加密",
    heading: "用公钥加密所选文件",
    icon: Lock,
    width: 540,
  },
  decrypt: {
    label: "解密",
    heading: "用私钥解密所选文件",
    icon: LockOpen,
    width: 500,
  },
  sign: {
    label: "签名",
    heading: "生成分离签名",
    icon: FileSignature,
    width: 520,
  },
  verify: {
    label: "验证",
    heading: "核验分离签名",
    icon: ShieldCheck,
    width: 500,
  },
  keys: { label: "公钥", heading: "本机公钥环", icon: KeyRound, width: 420 },
};

export function isAction(value: string | undefined): value is Action {
  return value !== undefined && value in ACTIONS;
}
