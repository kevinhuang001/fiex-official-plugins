#!/usr/bin/env python3
"""Exercise *released plugin binaries* against real WebDAV/SMB fixtures.

This suite belongs in fiex-official-plugins, never in Fiex Core.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile


def write_frame(stream, value):
    body = json.dumps(value, separators=(",", ":")).encode("utf-8")
    stream.write(b"Content-Length: " + str(len(body)).encode("ascii") + b"\r\n\r\n" + body)
    stream.flush()


def read_frame(stream):
    size = None
    while True:
        line = stream.readline()
        assert line, "plugin unexpectedly closed its control stream"
        if line == b"\r\n":
            break
        name, sep, value = line.partition(b":")
        assert sep, f"invalid plugin response header: {line!r}"
        if name.lower() == b"content-length":
            assert size is None, "repeated content length"
            size = int(value.strip())
            assert 0 <= size <= 4 * 1024 * 1024, f"invalid frame size {size}"
    assert size is not None
    data = stream.read(size)
    assert len(data) == size, "truncated plugin response"
    return json.loads(data)


class Plugin:
    def __init__(self, binary, plugin_id):
        self.child = subprocess.Popen([str(binary.resolve())],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.DEVNULL)
        self.serial = 0
        info = self.rpc("plugin.initialize", {"protocolVersion": "1.0"})
        assert info["pluginId"] == plugin_id and info["ready"] is True, info

    def rpc(self, method, params):
        self.serial += 1
        write_frame(self.child.stdin, {"jsonrpc": "2.0", "id": self.serial,
                                        "method": method, "params": params})
        response = read_frame(self.child.stdout)
        assert response.get("id") == self.serial, response
        assert "error" not in response, f"{method} failed: {response}"
        assert "result" in response, response
        return response["result"]

    def invalid(self, method, params):
        self.serial += 1
        write_frame(self.child.stdin, {"jsonrpc": "2.0", "id": self.serial,
                                        "method": method, "params": params})
        response = read_frame(self.child.stdout)
        assert response["id"] == self.serial, response
        assert response["error"]["code"] == -32602, response

    def close(self):
        try:
            self.rpc("plugin.shutdown", {})
            self.child.stdin.close()
            assert self.child.wait(timeout=10) == 0
        finally:
            if self.child.poll() is None:
                self.child.kill()
                self.child.wait(timeout=5)


def unpack(packages, slug, dest):
    candidates = list(packages.glob(slug + "-linux-x86_64.fiexplugin"))
    assert len(candidates) == 1, f"missing Linux package for {slug}"
    with zipfile.ZipFile(candidates[0]) as archive:
        # Packages were created by our own CI in an isolated temporary folder.
        archive.extractall(dest)
    manifest = json.loads((dest / "manifest.json").read_text(encoding="utf-8"))
    binary = dest / manifest["entrypoints"]["linux-x86_64"]
    assert binary.is_file()
    binary.chmod(0o755)
    return binary, manifest["id"]


def check_list(plugin, connection, description):
    assert plugin.rpc("connection.validate", {"connection": connection})["valid"] is True
    first = plugin.rpc("fs.list", {"connection": connection, "directoryId": "",
                                    "limit": 1})
    assert len(first["entries"]) == 1, first
    cursor = first["nextCursor"]
    assert cursor is not None, f"{description}: pagination not exercised"
    second = plugin.rpc("fs.list", {"connection": connection, "directoryId": "",
                                     "limit": 100, "cursor": cursor})
    combined = first["entries"] + second["entries"]
    names = [entry["name"] for entry in combined]
    assert "hello.txt" in names and "another.txt" in names, f"{description}: {combined}"
    assert len(names) == len(set(names)), "pagination yielded duplicated entries"
    plugin.invalid("fs.list", {"connection": connection, "directoryId": "../escape"})


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--packages", type=Path, required=True)
    args = p.parse_args()
    with tempfile.TemporaryDirectory() as workspace:
        root = Path(workspace)
        exe, plugin_id = unpack(args.packages, "webdav", root / "webdav")
        cfg = {"url": os.environ.get("FIEX_TEST_DAV_URL", "http://127.0.0.1:8080/")}
        plugin = Plugin(exe, plugin_id)
        try:
            check_list(plugin, cfg, "WebDAV")
            result = plugin.rpc("fs.readSmall", {"connection": cfg, "nodeId": "hello.txt", "maxBytes": 1024})
            assert base64.b64decode(result["base64"]) == b"hello webdav\n", result
            output = root / "webdav-download.txt"
            result = plugin.rpc("fs.download", {"connection": cfg, "nodeId": "hello.txt",
                                                 "destination": str(output.resolve())})
            assert result["bytesWritten"] == len(b"hello webdav\n"), result
            assert output.read_bytes() == b"hello webdav\n"
            download = root / "webdav" / "read-webdav.txt"
            reply = plugin.rpc("fs.download", {"connection": cfg,
                                               "nodeId": "hello.txt",
                                               "destination": str(download)})
            assert reply["bytesWritten"] == len(b"hello webdav\n")
            assert download.read_bytes() == b"hello webdav\n"
        finally:
            plugin.close()
        print("PASS WebDAV: validate, list, pagination, path guards, preview and full download", flush=True)

        exe, plugin_id = unpack(args.packages, "smb", root / "smb")
        cfg = {"host": "127.0.0.1", "share": "fiex", "base": "",
               "port": int(os.environ.get("FIEX_TEST_SMB_PORT", "445")),
               "username": "fiex",
               "password": os.environ["FIEX_TEST_SMB_PASSWORD"]}
        plugin = Plugin(exe, plugin_id)
        try:
            check_list(plugin, cfg, "SMB")
            result = plugin.rpc("fs.readSmall", {"connection": cfg,
                                                  "nodeId": "hello.txt", "maxBytes": 1024})
            assert base64.b64decode(result["base64"]) == b"hello smb\n", result
            output = root / "smb-download.txt"
            result = plugin.rpc("fs.download", {"connection": cfg, "nodeId": "hello.txt",
                                                 "destination": str(output.resolve())})
            assert result["bytesWritten"] == len(b"hello smb\n"), result
            assert output.read_bytes() == b"hello smb\n"
            download = root / "smb" / "read-smb.txt"
            reply = plugin.rpc("fs.download", {"connection": cfg,
                                               "nodeId": "hello.txt",
                                               "destination": str(download)})
            assert reply["bytesWritten"] == len(b"hello smb\n")
            assert download.read_bytes() == b"hello smb\n"
            plugin.invalid("fs.readSmall", {"connection": cfg, "nodeId": "../escape"})
        finally:
            plugin.close()
        print("PASS SMB: validate, list, pagination, path guards, preview and download", flush=True)


if __name__ == "__main__":
    main()
