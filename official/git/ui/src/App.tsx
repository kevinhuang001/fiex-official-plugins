import { useState } from "react";
import { FiexUiClient } from "../../../../sdk/ui/src/index";
import { CommitDialog } from "./components/CommitDialog";
import { CommitPicker } from "./components/CommitPicker";
import {
  BusyIndicator,
  EmptyFolderState,
  LoadingState,
  NotRepositoryState,
  PanelError,
} from "./components/PanelStates";
import { RemoteDialog } from "./components/RemoteDialog";
import { RepositoryHeader } from "./components/RepositoryHeader";
import { StatusList } from "./components/StatusList";
import { useGitActions } from "./hooks/useGitActions";
import { useGitChanges } from "./hooks/useGitChanges";
import { useGitHistory } from "./hooks/useGitHistory";
import { useGitRepository } from "./hooks/useGitRepository";
import { usePanelResize } from "./hooks/usePanelResize";
import { useRemotePanel } from "./hooks/useRemotePanel";

const client = new FiexUiClient();

/** Composition root: owns the bridge client and the panel's UI state. */
export function App() {
  const repository = useGitRepository(client);
  const { changes, staged, unstaged } = useGitChanges(repository.status);
  const history = useGitHistory(repository.history, repository.refs?.head);
  const actions = useGitActions(client, repository);
  const [message, setMessage] = useState("");
  const [commitOpen, setCommitOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [remoteOpen, setRemoteOpen] = useState(false);
  const remote = useRemotePanel(client, repository, actions, remoteOpen);

  usePanelResize(client, {
    status: repository.status,
    error: repository.error,
    busy: repository.busy,
    commitOpen,
    pickerOpen,
    remoteOpen,
  });

  if (!repository.loaded) {
    return (
      <div className="app">
        <LoadingState />
      </div>
    );
  }

  return (
    <div className="app">
      {repository.error && <PanelError message={repository.error} />}
      {repository.busy && <BusyIndicator />}
      {!repository.root && <EmptyFolderState />}
      {repository.root && !repository.status && (
        <NotRepositoryState busy={repository.busy} onInit={actions.init} />
      )}

      {repository.status && (
        <>
          <RepositoryHeader
            root={repository.root}
            status={repository.status}
            refs={repository.refs}
            busy={repository.busy}
            stagedCount={staged.length}
            onOpenCommit={() => setCommitOpen(true)}
            onOpenPicker={() => setPickerOpen(true)}
            onOpenRemote={() => setRemoteOpen(true)}
            onRefresh={() => void repository.load()}
          />
          <StatusList
            root={repository.root}
            changes={changes}
            staged={staged}
            unstaged={unstaged}
            busy={repository.busy}
            onToggle={actions.toggle}
          />
        </>
      )}

      {commitOpen && (
        <CommitDialog
          stagedCount={staged.length}
          message={message}
          identity={repository.identity}
          busy={repository.busy}
          error={repository.error}
          onMessageChange={setMessage}
          onIdentityChange={repository.setIdentity}
          onClose={() => setCommitOpen(false)}
          onSubmit={() =>
            actions.commit(message, () => {
              setMessage("");
              setCommitOpen(false);
            })
          }
        />
      )}

      {pickerOpen && (
        <CommitPicker
          entries={history}
          busy={repository.busy}
          onClose={() => setPickerOpen(false)}
          onSelect={actions.openCommit}
        />
      )}

      {remoteOpen && (
        <RemoteDialog
          remote={repository.remote}
          refs={repository.refs}
          remoteName={repository.remoteName}
          busy={repository.busy}
          editing={remote.editing}
          pendingRemove={remote.pendingRemove}
          syncBranch={remote.syncBranch}
          onEditingChange={remote.setEditing}
          onPendingRemoveChange={remote.setPendingRemove}
          onSyncBranchChange={remote.setSyncBranch}
          onSelectRemote={repository.setRemoteName}
          onSaveRemote={remote.save}
          onRemoveRemote={remote.remove}
          onSync={remote.sync}
          onClose={() => setRemoteOpen(false)}
        />
      )}
    </div>
  );
}
