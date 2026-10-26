export interface Account {
  connectionId: string;
  email: string;
  imapHost: string;
  smtpHost: string;
}

export interface Context {
  source?: string;
  connectionId?: string;
  state?: Partial<Account>;
}

export interface Header {
  seq: number;
  subject: string;
  from: string;
  date: string;
}

export interface Body extends Header {
  body: string;
}

export type Status = { kind: "idle" | "busy" | "error" | "ok"; text?: string };
