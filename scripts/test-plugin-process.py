#!/usr/bin/env python3
"""End-to-end, black-box, real executable tests for all Fiex plugin processes."""
import argparse
import json
from pathlib import Path
import subprocess

def send(out, obj):
    data = json.dumps(obj, ensure_ascii=False).encode("utf-8")
    out.write(b"Content-Length: " + str(len(data)).encode() + b"\r\n\r\n" + data)
    out.flush()

def receive(inp):
    size = None
    while True:
        line = inp.readline()
        if not line:
            raise AssertionError("plugin unexpectedly closed stdout")
        if line == b"\r\n":
            break
        key, colon, value = line.partition(b":")
        assert colon
        if key.lower() == b"content-length":
            size = int(value.strip())
            assert 0 <= size <= 4 * 1024 * 1024
    assert size is not None
    data = inp.read(size)
    assert len(data) == size
    return json.loads(data)

def run(binary, plugin_id):
    with subprocess.Popen([str(binary.resolve())], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as child:
        for req, check in [
            ({"id":1,"method":"plugin.initialize","params":{"protocolVersion":"6.0"}}, "init"),
            ({"id":2,"method":"plugin.ping","params":{}}, "ping"),
            ({"id":3,"method":"test.unknown","params":{}}, "unknown"),
            ({"id":4,"method":"plugin.shutdown","params":{}}, "shutdown"),
        ]:
            send(child.stdin, {"jsonrpc":"2.0",**req})
            resp = receive(child.stdout)
            assert resp.get("id") == req["id"], resp
            if check == "init":
                assert resp["result"]["pluginId"] == plugin_id and resp["result"]["ready"], resp
            elif check == "ping":
                assert resp["result"]["alive"], resp
            elif check == "unknown":
                assert resp["error"]["code"] == -32601, resp
            else:
                assert "result" in resp, resp
        child.stdin.close()
        assert child.wait(timeout=10) == 0

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--binary-root", required=True)
    args = parser.parse_args()
    for slug in ("ssh","git","webdav","gdrive","smb","openlist","office-preview","data-preview","archive","checksum","everything","gpg","email"):
        manifest = json.loads((Path("official") / slug / "manifest.json").read_text(encoding="utf-8"))
        entry = manifest.get("runtime", {}).get("entrypoints", {}).get(args.target)
        if not entry:
            print("SKIP", slug, "for", args.target)
            continue
        binary = Path(args.binary_root) / entry
        assert binary.is_file(), binary
        run(binary, manifest["id"])
        print("PASS", slug, "real process IPC")
if __name__ == "__main__":
    main()
