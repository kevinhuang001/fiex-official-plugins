use super::connection::*;
use super::*;
fn open(ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionOpenResult, RpcError> {
    let mount = ctx
        .input()?
        .connection_request()
        .and_then(|request| request.mount_id.as_deref())
        .ok_or_else(|| RpcError::invalid_params("missing mountId"))?;
    Ok(sdk::ConnectionOpenResult::Vfs {
        mount_id: mount.into(),
        node_ref: sdk::ResourceRef {
            mount_id: mount.into(),
            node_id: String::new(),
        },
    })
}

fn validate(ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionValidation, RpcError> {
    let connection = ctx
        .input()?
        .connection_request()
        .ok_or_else(|| RpcError::invalid_params("missing connection request"))?
        .connection_value()
        .map_err(RpcError::invalid_params)?
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let cfg: StoredConnection =
        serde_json::from_value(connection).map_err(|e| RpcError::invalid_params(e.to_string()))?;
    safe_relative(&cfg.base)?;
    directory_entries(&ctx.tcp(), &cfg, "")?;
    Ok(sdk::ConnectionValidation::valid())
}

pub(super) struct Smb;

impl VfsProvider for Smb {
    fn discover(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<Vec<sdk::filesystem::FsOperation>, RpcError> {
        discovery::discover(ctx)
    }

    fn operations(&self) -> &'static [sdk::filesystem::FsOperation] {
        &[
            sdk::filesystem::FsOperation::List,
            sdk::filesystem::FsOperation::Stat,
            sdk::filesystem::FsOperation::Read,
            sdk::filesystem::FsOperation::Write,
            sdk::filesystem::FsOperation::Mkdir,
            sdk::filesystem::FsOperation::Rename,
            sdk::filesystem::FsOperation::Remove,
        ]
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        list(request)
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        stat(request)
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        read(request)
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        sync::write(request)
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        mkdir(request)
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        rename(request)
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), RpcError> {
        remove(request)
    }
}

impl sdk::ConnectionProvider for Smb {
    fn open(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::ConnectionOpenResult, sdk::RpcError> {
        open(ctx)
    }
    fn validate(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::ConnectionValidation, sdk::RpcError> {
        validate(ctx)
    }
}

pub(super) fn run() {
    let providers = sdk::Providers::new()
        .diagnostics()
        .filesystem(Smb)
        .connection(Smb);
    if let Err(error) = providers.serve("org.fiex.smb") {
        eprintln!(
            "SMB plugin IPC failed: transport closed ({:?})",
            error.kind()
        );
        std::process::exit(1);
    }
}
