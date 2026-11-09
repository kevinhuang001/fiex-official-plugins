use super::connection::*;
use super::*;
pub(super) fn read(request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
    let cfg = connection(request)?;
    let node = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing node ID"))?;
    safe_relative(node)?;
    safe_relative(&cfg.base)?;
    read_stream(&request.ctx().tcp(), &cfg, node)
}
