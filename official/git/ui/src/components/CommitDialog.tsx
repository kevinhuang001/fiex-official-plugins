import { GitCommitHorizontal, X } from "lucide-react";
import type { Identity } from "../types";
import { FormError } from "./PanelStates";

export function CommitDialog({
  stagedCount,
  message,
  identity,
  busy,
  error,
  onMessageChange,
  onIdentityChange,
  onClose,
  onSubmit,
}: {
  stagedCount: number;
  message: string;
  identity: Identity;
  busy: boolean;
  error: string;
  onMessageChange: (message: string) => void;
  onIdentityChange: (identity: Identity) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  return (
    <div className="git-sheet" role="dialog" aria-label="提交修改">
      <form
        className="git-sheet-card git-commit-card"
        onSubmit={(event) => {
          event.preventDefault();
          if (
            !busy &&
            stagedCount &&
            message.trim() &&
            identity.name &&
            identity.email
          )
            onSubmit();
        }}
      >
        <header>
          <strong>提交修改</strong>
          <button type="button" aria-label="关闭" onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="git-commit-form">
          <label htmlFor="git-message">提交说明 / comment</label>
          <textarea
            id="git-message"
            aria-label="提交说明"
            placeholder="提交说明"
            value={message}
            required
            rows={4}
            onChange={(event) => onMessageChange(event.target.value)}
          />
          <fieldset className="git-identity">
            <legend>提交身份</legend>
            <div>
              <label>
                姓名
                <input
                  aria-label="提交者姓名"
                  value={identity.name}
                  required
                  placeholder="姓名"
                  onChange={(event) =>
                    onIdentityChange({ ...identity, name: event.target.value })
                  }
                />
              </label>
              <label>
                邮箱
                <input
                  aria-label="提交者邮箱"
                  type="email"
                  value={identity.email}
                  required
                  placeholder="you@example.com"
                  onChange={(event) =>
                    onIdentityChange({ ...identity, email: event.target.value })
                  }
                />
              </label>
            </div>
          </fieldset>
          {error && <FormError message={error} />}
          <button
            className="primary"
            type="submit"
            disabled={
              busy ||
              !stagedCount ||
              !message.trim() ||
              !identity.name ||
              !identity.email
            }
          >
            <GitCommitHorizontal size={15} />
            {stagedCount
              ? `提交 ${stagedCount} 个文件`
              : "没有已暂存的文件"}
          </button>
        </div>
      </form>
    </div>
  );
}
