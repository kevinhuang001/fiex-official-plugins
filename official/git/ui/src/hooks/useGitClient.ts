import type { FiexUiClient } from "../../../../../sdk/ui/src/index";

/** Single bridge call shape used by every repository request. */
export const invokeGit = <T>(
  client: FiexUiClient,
  method: string,
  params: Record<string, unknown> = {},
) => client.invoke<T>("plugin.invoke", { method, params });
