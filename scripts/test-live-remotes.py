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
import sys
import tempfile
import zipfile
import socket
import urllib.request
import urllib.error
import urllib.parse


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
                                      stderr=subprocess.PIPE)
        self.upload_source = b"connection sync roundtrip\n"
        self.serial = 0
        self.resources = {}
        self.next_resource = 0
        info = self.rpc("plugin.initialize", {"protocolVersion": "2.0"})
        assert info["pluginId"] == plugin_id and info["ready"] is True, info

    def rpc(self, method, params):
        self.serial += 1
        write_frame(self.child.stdin, {"jsonrpc": "2.0", "id": self.serial,
                                        "method": method, "params": params})
        response = self.response()
        assert response.get("id") == self.serial, response
        assert "error" not in response, f"{method} failed: {response}"
        assert "result" in response, response
        return response["result"]

    def response(self):
        while True:
            value = read_frame(self.child.stdout)
            if not str(value.get("method", "")).startswith("host."):
                return value
            try:
                result = self.host_call(value["method"], value.get("params", {}))
                reply = {"jsonrpc": "2.0", "id": value["id"], "result": result}
            except Exception as error:
                reply = {"jsonrpc": "2.0", "id": value["id"], "error": {"code": -32001, "message": str(error)}}
            write_frame(self.child.stdin, reply)

    def host_call(self, method, params):
        # Test fixture broker only. Production authorization is tested separately
        # through the actual Core host in the core-integration job.
        if method in {"host.task.start", "host.task.progress", "host.task.finish", "host.task.cancel"}:
            return None
        if method == "host.fs.openRead":
            assert params["path"] == "source.dat"
            return {"handle":"snapshot", "size":len(self.upload_source)}
        if method == "host.fs.readOpen":
            assert params["handle"] == "snapshot"
            data=self.upload_source[params["offset"]:params["offset"]+params["length"]]
            return {"base64":base64.b64encode(data).decode(),"size":len(data),"eof":params["offset"]+len(data)>=len(self.upload_source)}
        if method == "host.fs.closeRead": return None
        if method == "host.net.http.open":
            url = params["url"]
            assert urllib.parse.urlparse(url).hostname in {"localhost", "127.0.0.1", "::1"}
            request = urllib.request.Request(url, data=(self.upload_source if params.get("bodyReadHandle") == "snapshot" else base64.b64decode(params.get("bodyBase64", "")) or None), headers=params.get("headers", {}), method=params.get("method", "GET"))
            try:
                response = urllib.request.urlopen(request, timeout=30)
            except urllib.error.HTTPError as error:
                response = error
            self.next_resource += 1
            key = str(self.next_resource)
            self.resources[key] = response
            return {"stream": key, "status": response.status, "headers": {k.lower(): v for k, v in response.headers.items()}}
        if method == "host.net.tcp.open":
            assert params["host"] in {"localhost", "127.0.0.1", "::1"}
            connection = socket.create_connection((params["host"], params["port"]), timeout=30)
            self.next_resource += 1
            key = str(self.next_resource)
            self.resources[key] = connection
            peer = connection.getpeername()
            return {"stream": key, "peerAddress": f"{peer[0]}:{peer[1]}"}
        if method in {"host.net.http.read", "host.net.tcp.readBytes"}:
            resource = self.resources[params["stream"]]
            size = min(params.get("size", 65536), 262144)
            data = resource.recv(size) if isinstance(resource, socket.socket) else resource.read(size)
            return {"base64": base64.b64encode(data).decode("ascii"), "size": len(data), "eof": not data}
        if method == "host.net.tcp.writeBytes":
            data = base64.b64decode(params["base64"])
            self.resources[params["stream"]].sendall(data)
            return {"size": len(data)}
        if method in {"host.net.http.close", "host.net.tcp.close"}:
            self.resources.pop(params["stream"]).close()
            return None
        raise AssertionError("unexpected host method: " + method)

    def invalid(self, method, params):
        self.serial += 1
        write_frame(self.child.stdin, {"jsonrpc": "2.0", "id": self.serial,
                                        "method": method, "params": params})
        response = self.response()
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
            for resource in self.resources.values():
                resource.close()
            self.resources.clear()
            if self.child.stderr is not None:
                diagnostics = self.child.stderr.read()
                if diagnostics:
                    print(diagnostics.decode("utf-8", "replace"), file=sys.stderr)


def unpack(packages, slug, dest):
    candidates = list(packages.glob(slug + "-linux-x86_64.fiexplugin"))
    assert len(candidates) == 1, f"missing Linux package for {slug}"
    with zipfile.ZipFile(candidates[0]) as archive:
        # Packages were created by our own CI in an isolated temporary folder.
        archive.extractall(dest)
    manifest = json.loads((dest / "manifest.json").read_text(encoding="utf-8"))
    binary = dest / manifest["runtime"]["entrypoints"]["linux-x86_64"]
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
    p.add_argument("--only", choices=["webdav", "smb", "both"], default="both")
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
            opened = plugin.rpc("fs.openRead", {"connection": cfg, "nodeId": "hello.txt"})
            stream_id = opened["streamId"]
            reply = plugin.rpc("fs.readStream", {"streamId": stream_id, "offset": 0,
                                                 "maxBytes": 262144})
            assert base64.b64decode(reply["base64"]) == b"hello webdav\n"
            assert reply["eof"] is True
            plugin.rpc("fs.closeRead", {"streamId": stream_id})
            baseline=plugin.rpc("fs.stat", {"connection":cfg,"nodeId":"hello.txt"})["revision"]
            uploaded=plugin.rpc("fs.writeFile", {"connection":cfg,"nodeId":"hello.txt","source":"source.dat","ifRevision":baseline})
            assert uploaded["revision"] != baseline
            verified=plugin.rpc("fs.readSmall", {"connection":cfg,"nodeId":"hello.txt","maxBytes":1024})
            assert base64.b64decode(verified["base64"]) == plugin.upload_source
            plugin.upload_source=base64.b64decode(result["base64"])
            plugin.rpc("fs.writeFile", {"connection":cfg,"nodeId":"hello.txt","source":"source.dat","ifRevision":uploaded["revision"]})
            plugin.invalid("fs.openRead", {"connection": cfg, "nodeId": "../escape"})
            plugin.invalid("fs.readStream", {"streamId": "missing", "offset": 0,
                                             "maxBytes": 1024})
            plugin.invalid("fs.readStream", {"streamId": stream_id, "offset": 0,
                                             "maxBytes": 262145})
        finally:
            plugin.close()
        print("PASS WebDAV: validate, list, pagination, path guards, preview and bounded byte streaming", flush=True)

        if args.only == "webdav":
            return
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
            opened = plugin.rpc("fs.openRead", {"connection": cfg, "nodeId": "hello.txt"})
            stream_id = opened["streamId"]
            reply = plugin.rpc("fs.readStream", {"streamId": stream_id, "offset": 0,
                                                 "maxBytes": 262144})
            assert base64.b64decode(reply["base64"]) == b"hello smb\n"
            assert reply["eof"] is True
            plugin.rpc("fs.closeRead", {"streamId": stream_id})
            baseline=plugin.rpc("fs.stat", {"connection":cfg,"nodeId":"hello.txt"})["revision"]
            uploaded=plugin.rpc("fs.writeFile", {"connection":cfg,"nodeId":"hello.txt","source":"source.dat","ifRevision":baseline})
            assert uploaded["revision"] != baseline
            verified=plugin.rpc("fs.readSmall", {"connection":cfg,"nodeId":"hello.txt","maxBytes":1024})
            assert base64.b64decode(verified["base64"]) == plugin.upload_source
            plugin.upload_source=base64.b64decode(result["base64"])
            plugin.rpc("fs.writeFile", {"connection":cfg,"nodeId":"hello.txt","source":"source.dat","ifRevision":uploaded["revision"]})
            plugin.invalid("fs.openRead", {"connection": cfg, "nodeId": "../escape"})
            plugin.invalid("fs.readStream", {"streamId": "missing", "offset": 0,
                                             "maxBytes": 1024})
            plugin.invalid("fs.readStream", {"streamId": stream_id, "offset": 0,
                                             "maxBytes": 262145})
            plugin.invalid("fs.readSmall", {"connection": cfg, "nodeId": "../escape"})
        finally:
            plugin.close()
        print("PASS SMB: validate, list, pagination, path guards, preview and bounded byte streaming", flush=True)


if __name__ == "__main__":
    main()
