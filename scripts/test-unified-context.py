#!/usr/bin/env python3
"""Exercise the real checksum process with a ref-only standard event and a host stub."""
import argparse
import base64
import importlib.util
import subprocess
from pathlib import Path

spec = importlib.util.spec_from_file_location("process_test", Path(__file__).with_name("test-plugin-process.py"))
protocol = importlib.util.module_from_spec(spec)
spec.loader.exec_module(protocol)


def run(binary):
    binary = Path(binary)
    if not binary.exists() and binary.with_suffix(".exe").exists():
        binary = binary.with_suffix(".exe")
    reference = {"mountId": "local", "nodeId": "opaque-selected-file"}
    with subprocess.Popen([str(Path(binary).resolve())], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
        protocol.send(child.stdin, {"jsonrpc": "2.0", "id": 1, "method": "plugin.initialize",
                                  "params": {"protocolVersion": "3.0"}})
        assert protocol.receive(child.stdout)["result"]["ready"]
        # No filename/metadata/read token is needed to hash a selected resource.
        protocol.send(child.stdin, {"jsonrpc": "2.0", "id": 2, "method": "info.resolve", "params": {
            "target": {"ref": reference}, "selection": [{"ref": reference}], "options": {}}})
        opened = False
        while True:
            response = protocol.receive(child.stdout)
            method = response.get("method")
            if method is None:
                assert response.get("id") == 2, response
                rows = response["result"]["sections"][0]["rows"]
                hashes = {row["label"]: row["value"] for row in rows}
                assert opened and hashes["SHA-256"] == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", response
                break
            params = response["params"]
            assert "authorization" in params
            if method == "host.buffer.create":
                # Exercise the SDK's ordinary inline fallback when shared memory is unavailable.
                protocol.send(child.stdin, {"jsonrpc": "2.0", "id": response["id"],
                                           "error": {"code": -32601, "message": "shared memory unavailable in host stub"}})
                continue
            if method == "host.fs.openRead":
                assert params["ref"] == reference and "path" not in params
                opened = True
                result = {"handle": "selected-reader", "size": 3}
            elif method == "host.fs.readOpen":
                assert params["handle"] == "selected-reader"
                data = b"abc"[params["offset"]:params["offset"] + params["length"]]
                result = {"base64": base64.b64encode(data).decode(), "size": len(data), "eof": True}
            elif method == "host.fs.closeRead":
                result = None
            else:
                raise AssertionError(f"unexpected host call {method}")
            protocol.send(child.stdin, {"jsonrpc": "2.0", "id": response["id"], "result": result})
        protocol.send(child.stdin, {"jsonrpc": "2.0", "id": 3, "method": "plugin.shutdown", "params": {}})
        assert protocol.receive(child.stdout)["id"] == 3
        child.stdin.close()
        assert child.wait(timeout=10) == 0
    print("PASS: ref-only Info event through real SDK process and authorized streaming IPC")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    run(parser.parse_args().binary)
