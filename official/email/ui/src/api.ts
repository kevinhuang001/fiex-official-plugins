import { FiexUiClient } from "../../../../sdk/ui/src/index";
import type { Account, Body, Header } from "./types";

export const client = new FiexUiClient();

function params(account: Account) {
  return {
    connectionId: account.connectionId,
    email: account.email,
    imapHost: account.imapHost,
    smtpHost: account.smtpHost,
  };
}

async function invoke<T>(method: string, input: Record<string, unknown>) {
  return client.invoke<T>("plugin.invoke", { method, params: input });
}

export function list(account: Account, limit = 20) {
  return invoke<Header[]>("email.list", { ...params(account), limit });
}

export function read(account: Account, seq: number) {
  return invoke<Body>("email.read", { ...params(account), seq });
}

export function send(account: Account, to: string, subject: string, body: string) {
  return invoke<{ ok: boolean }>("email.send", {
    ...params(account),
    to,
    subject,
    body,
  });
}
