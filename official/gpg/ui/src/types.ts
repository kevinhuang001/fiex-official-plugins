import { t } from "../../../../support/ui/i18n";
import {
  FileSignature,
  KeyRound,
  Lock,
  LockOpen,
  ShieldCheck,
} from "lucide-react";

export type Action = "encrypt" | "decrypt" | "sign" | "verify" | "keys";

export type Key = {
  fingerprint: string;
  keyId: string;
  name: string;
  email: string;
  expires: string;
  /** The user's keyring has the matching secret key. */
  hasSecret?: boolean;
};

export type CreateKeyInput = {
  name: string;
  email: string;
  expiry: string;
  passphrase: string;
};

export type Status = { kind: "idle" | "busy" | "ok" | "error"; text: string };

export type ActionMeta = {
  label: string;
  heading: string;
  icon: typeof Lock;
  width: number;
};

export const DEFAULT_ACTION: Action = "keys";

export const getActions = (): Record<Action, ActionMeta> => ({
  encrypt: {
    label: t("加密"),
    heading: t("用公钥加密所选文件"),
    icon: Lock,
    width: 540,
  },
  decrypt: {
    label: t("解密"),
    heading: t("用私钥解密所选文件"),
    icon: LockOpen,
    width: 500,
  },
  sign: {
    label: t("签名"),
    heading: t("生成分离签名"),
    icon: FileSignature,
    width: 520,
  },
  verify: {
    label: t("验证"),
    heading: t("核验分离签名"),
    icon: ShieldCheck,
    width: 500,
  },
  keys: {
    label: t("GPG 钥匙串"),
    heading: t("本机公钥环 · 导入、导出与查看密钥"),
    icon: KeyRound,
    width: 620,
  },
});

export function isAction(value: string | undefined): value is Action {
  return value !== undefined && value in getActions();
}
