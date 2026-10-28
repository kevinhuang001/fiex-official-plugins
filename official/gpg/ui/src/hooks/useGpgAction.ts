import { t } from "../../../../../support/ui/i18n";
import type { ResourceContext } from "../../../../../sdk/ui/src/index";
import { useState } from "react";
import { client } from "../client";
import { baseName, outputName } from "../paths";
import type { Action, Status } from "../types";

type Options = {
  action: Action;
  directory: ResourceContext | undefined;
  files: ResourceContext[];
  data: ResourceContext | undefined;
  signature: ResourceContext | undefined;
  fingerprint: string;
  armor: boolean;
  trust: boolean;
  setStatus: (status: Status) => void;
};

export function useGpgAction({
  action,
  directory,
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
    const inputs = (action === "verify" ? [data] : files).filter(
      (resource): resource is ResourceContext => Boolean(resource),
    );
    if (!inputs.length) {
      setStatus({ kind: "error", text: t("请选择要处理的文件") });
      return;
    }
    if (action === "verify" && !signature) {
      setStatus({ kind: "error", text: t("请选择签名文件") });
      return;
    }
    if ((action === "encrypt" || action === "sign") && !fingerprint) {
      setStatus({ kind: "error", text: t("请选择密钥") });
      return;
    }
    setStatus({ kind: "busy", text: "" });
    setProgress(0);
    let done = 0;
    const failures: string[] = [];
    for (const input of inputs) {
      setStatus({ kind: "busy", text: t("正在处理 {0}", baseName(input)) });
      try {
        if (action === "verify") {
          const result = await client.plugin.invoke<{
            signer: string;
            fingerprint: string;
          }>("gpg.verify", { signature: signature!.ref, input: input.ref });
          setStatus({
            kind: "ok",
            text: t(
              "签名有效{0}",
              result.signer ? ` · 签名者 ${result.signer}` : "",
            ),
          });
          done += 1;
          continue;
        }
        const output = await client.dialog.saveFile({
          directory: directory?.ref,
          suggestedName: outputName(input, action, armor),
        });
        if (!output) {
          setStatus({ kind: "idle", text: "" });
          return;
        }
        await client.plugin.invoke(`gpg.${action}`, {
          input: input.ref,
          output: output.ref,
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
      setStatus({ kind: "ok", text: t("已完成 {0} 个文件", done) });
    }
  };

  return { progress, run };
}
