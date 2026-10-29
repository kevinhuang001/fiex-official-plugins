import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { client, i18n, t, ready } from "../../../../support/ui/i18n";
import "@xterm/xterm/css/xterm.css";
import "./style.css";

type Settings = {
  fontFamily?: string;
  fontSize?: number;
};
type ThemeSnapshot = {
  mode?: "light" | "dark";
  tokens?: Record<string, string>;
};

const root = document.getElementById("root")!;
const status = document.createElement("div");
status.role = "status";
let statusSource = "正在连接…";
const paintStatus = () => {
  status.textContent = t(statusSource);
};
i18n.subscribe(paintStatus);
paintStatus();
const host = document.createElement("div");
host.className = "terminal";
root.append(status, host);
const terminal = new Terminal({
  cursorBlink: true,
  fontSize: 14,
  fontFamily: "Menlo, Consolas, monospace",
  scrollback: 5000,
});
const fit = new FitAddon();
terminal.loadAddon(fit);
terminal.open(host);
let sessionId = "";
let stopped = false;
let pending: number[] = [];
let settings: Settings = {};
const encoder = new TextEncoder();
const invoke = <T>(method: string, params: Record<string, unknown>) =>
  client.invoke<T>("plugin.invoke", { method, params });

/**
 * Terminal colors belong to the host theme: chrome colors come from the theme
 * tokens, and the fixed 16-color ANSI palette comes from the host tokens when
 * present (Core forwards `--terminal-ansi-*`) or this panel's stylesheet
 * otherwise. There is no per-plugin color setting to keep in sync.
 */
function applyTheme(snapshot: ThemeSnapshot | null) {
  const tokens = snapshot?.tokens || {};
  const dark = snapshot
    ? snapshot.mode === "dark"
    : matchMedia("(prefers-color-scheme: dark)").matches;
  const background = tokens.surface || tokens.canvas || (dark ? "#141820" : "#ffffff");
  const foreground = tokens.text || (dark ? "#e6edf3" : "#1f2328");
  const cursor = tokens.accent || foreground;
  const selection = tokens.selected || tokens.accent || foreground;
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
  document.documentElement.style.setProperty("--terminal-bg", background);
  document.documentElement.style.setProperty("--terminal-fg", foreground);
  document.documentElement.style.setProperty("--terminal-muted", tokens.muted || foreground);
  document.documentElement.style.setProperty("--terminal-line", tokens.line || "transparent");
  const style = getComputedStyle(document.documentElement);
  const ansi = (name: string) =>
    tokens[`terminal-ansi-${name}`]?.trim() ||
    style.getPropertyValue(`--terminal-ansi-${name}`).trim() ||
    undefined;
  terminal.options.theme = {
    background,
    foreground,
    cursor,
    cursorAccent: background,
    selectionBackground: selection,
    black: ansi("black"),
    red: ansi("red"),
    green: ansi("green"),
    yellow: ansi("yellow"),
    blue: ansi("blue"),
    magenta: ansi("magenta"),
    cyan: ansi("cyan"),
    white: ansi("white"),
    brightBlack: ansi("bright-black"),
    brightRed: ansi("bright-red"),
    brightGreen: ansi("bright-green"),
    brightYellow: ansi("bright-yellow"),
    brightBlue: ansi("bright-blue"),
    brightMagenta: ansi("bright-magenta"),
    brightCyan: ansi("bright-cyan"),
    brightWhite: ansi("bright-white"),
  };
  const family = settings.fontFamily?.trim() || tokens["font-family"] || "Menlo, Consolas, monospace";
  const size = Math.min(32, Math.max(10, Number(settings.fontSize) || 14));
  terminal.options.fontFamily = family;
  terminal.options.fontSize = size;
  if (host.clientWidth && host.clientHeight) fit.fit();
}

const input = terminal.onData((text) => {
  const bytes = Array.from(encoder.encode(text));
  if (pending.length + bytes.length <= 65536) pending.push(...bytes);
});
const resize = new ResizeObserver(() => {
  if (host.clientWidth && host.clientHeight) fit.fit();
});
resize.observe(host);
function close() {
  if (stopped) return;
  stopped = true;
  resize.disconnect();
  input.dispose();
  if (sessionId) void invoke("ssh.close", { sessionId }).catch(() => {});
  terminal.dispose();
}
window.addEventListener("pagehide", close, { once: true });
type ConnectionContext = {
  state?: { sessionId?: string };
  settings?: Settings;
};
async function run(initial?: ConnectionContext) {
  const context = initial ?? (await client.invoke<ConnectionContext>("context.get"));
  sessionId = context.state?.sessionId || "";
  if (!sessionId) throw new Error(t("请从连接中打开 SSH"));
  if (stopped) {
    void invoke("ssh.close", { sessionId });
    return;
  }
  fit.fit();
  await invoke("ssh.start", {
    sessionId,
    cols: terminal.cols,
    rows: terminal.rows,
  });
  terminal.focus();
  while (!stopped) {
    // Only one exchange in flight; input and output remain ordered.
    const data = pending;
    pending = [];
    const value = await invoke<{
      dataBase64: string;
      ready: boolean;
      ended: boolean;
      error?: string;
    }>("ssh.exchange", {
      sessionId,
      dataBase64: btoa(String.fromCharCode(...data)),
      cols: terminal.cols,
      rows: terminal.rows,
    });
    if (stopped) return;
    if (value.dataBase64)
      terminal.write(Uint8Array.from(atob(value.dataBase64), (char) => char.charCodeAt(0)));
    statusSource =
      value.error || (value.ended ? "已断开连接" : value.ready ? "" : "正在连接…");
    paintStatus();
    if (value.ended) {
      await invoke("ssh.close", { sessionId });
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 24));
  }
}
void ready
  .then(async () => {
    const [rawTheme, context] = await Promise.all([
      client.theme.get().catch(() => null),
      client.invoke<ConnectionContext>("context.get"),
    ]);
    settings = context.settings || {};
    applyTheme(rawTheme as ThemeSnapshot | null);
    client.theme.subscribe((next) => applyTheme(next as ThemeSnapshot));
    await run(context);
  })
  .catch((error) => {
    statusSource = String(error);
    paintStatus();
    if (sessionId) void invoke("ssh.close", { sessionId }).catch(() => {});
  });
