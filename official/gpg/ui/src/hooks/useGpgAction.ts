import { useState } from "react";
import { client } from "../client";
import { baseName, directoryName, join, outputName } from "../paths";
import type { Action, Status } from "../types";

type Options = {
  action: Action;
  files: string[];
  data: string;
  signature: string;
  fingerprint: string;
  armor: boolean;
  trust: boolean;
  setStatus: (status: Status) => void;
};

export function useGpgAction({
  action,
  files,
  data,
  signature,
  fingerprint,
  armor,
  trust,
  setStatus,
}: Options) {
  const [progress, setProgress] = useState(0);

  const run = async () => {
    const inputs = (action === "verify" ? [data] : files).filter(Boolean);
    if (!inputs.length) {
      setStatus({ kind: "error", text: "请选择要处理的文件" });
      return;
    }
    if (action === "verify" && !signature) {
      setStatus({ kind: "error", text: "请选择签名文件" });
      return;
    }
    if ((action === "encrypt" || action === "sign") && !fingerprint) {
      setStatus({ kind: "error", text: "请选择密钥" });
      return;
    }
    setStatus({ kind: "busy", text: "" });
    setProgress(0);
    let done = 0;
    const failures: string[] = [];
    for (const input of inputs) {
      setStatus({ kind: "busy", text: `正在处理 ${baseName(input)}` });
      try {
        if (action === "verify") {
          const result = await client.plugin.invoke<{
            signer: string;
            fingerprint: string;
          }>("gpg.verify", { signature, input });
          setStatus({
            kind: "ok",
            text: `签名有效${result.signer ? ` · 签名者 ${result.signer}` : ""}`,
          });
          done += 1;
          continue;
        }
        const output = join(
          directoryName(input),
          outputName(input, action, armor),
        );
        await client.plugin.invoke(`gpg.${action}`, {
          input,
          output,
          fingerprint,
          armor,
          trustUnverified: trust,
        });
        done += 1;
      } catch (error) {
        failures.push(`${baseName(input)}：${String(error)}`);
      }
      setProgress(done);
    }
    if (failures.length) {
      setStatus({ kind: "error", text: failures[0] });
    } else if (action !== "verify") {
      setStatus({ kind: "ok", text: `已完成 ${done} 个文件` });
    }
  };

  return { progress, run };
}
