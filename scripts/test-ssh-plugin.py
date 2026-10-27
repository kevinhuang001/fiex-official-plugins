#!/usr/bin/env python3
"""Black-box SSH plugin integration test.

The script acts as the Host RPC endpoint: it brokers TCP bytes, records terminal
output, and feeds terminal input. It then starts a real shell command and waits
for its output. Run it against a local SSH server and an already-built plugin.
"""
import argparse
import base64
import json
from pathlib import Path
import queue
import socket
import subprocess
import threading
import time

READ_CHUNK = 65536


def send(stream, lock, value):
    data = json.dumps(value, ensure_ascii=False).encode("utf-8")
    with lock:
        stream.write(b"Content-Length: " + str(len(data)).encode() + b"\r\n\r\n" + data)
        stream.flush()


def receive(stream):
    size = None
    while True:
        line = stream.readline()
        if not line:
            raise AssertionError("SSH plugin closed stdout")
        if line == b"\r\n":
            break
        key, colon, value = line.partition(b":")
        assert colon, line
        if key.lower() == b"content-length":
            size = int(value.strip())
            assert 0 <= size <= 4 * 1024 * 1024
    assert size is not None
    data = stream.read(size)
    assert len(data) == size
    return json.loads(data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=2222)
    parser.add_argument("--user", default="root")
    parser.add_argument("--identity", required=True)
    args = parser.parse_args()

    child = subprocess.Popen(
        [str(Path(args.binary).resolve())],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    send_lock = threading.Lock()
    send(child.stdin, send_lock, {
        "jsonrpc": "2.0", "id": 1,
        "method": "plugin.initialize",
        "params": {"protocolVersion": "4.0"},
    })
    init = receive(child.stdout)
    assert init["result"]["pluginId"] == "org.fiex.ssh", init

    sock = None
    output = bytearray()
    input_queue = bytearray()
    read_queue = queue.Queue()
    finished = threading.Event()
    command_sent = False

    def reader():
        try:
            while True:
                data = sock.recv(READ_CHUNK)
                read_queue.put(data)
                if not data:
                    break
        except Exception:
            read_queue.put(None)

    def respond_read(request_id, size):
        try:
            data = read_queue.get(timeout=30)
        except queue.Empty:
            data = b""
        if data is None:
            data = b""
        send(child.stdin, send_lock, {
            "jsonrpc": "2.0", "id": request_id,
            "result": {"base64": base64.b64encode(data[:size]).decode()},
        })

    send(child.stdin, send_lock, {
        "jsonrpc": "2.0", "id": 2,
        "method": "terminal.open",
        "params": {
            "sessionId": "ssh-integration",
            "cols": 100,
            "rows": 30,
            "connection": {
                "host": args.host,
                "port": args.port,
                "username": args.user,
                "identityFile": args.identity,
            },
        },
    })

    deadline = time.time() + 45
    while time.time() < deadline and not finished.is_set():
        try:
            message = receive(child.stdout)
        except AssertionError:
            break
        method = message.get("method")
        request_id = message.get("id")
        params = message.get("params") or {}
        if method is None:
            continue

        if method == "host.task.start":
            result = {"taskId": params.get("taskId")}
        elif method in ("host.task.finish", "host.task.progress"):
            result = None
        elif method == "host.net.tcp.open":
            sock = socket.create_connection((args.host, args.port), timeout=10)
            threading.Thread(target=reader, daemon=True).start()
            result = {"stream": "ssh-integration", "peerAddress": f"{args.host}:{args.port}"}
        elif method == "host.net.tcp.readBytes":
            threading.Thread(
                target=respond_read,
                args=(request_id, int(params.get("size", READ_CHUNK))),
                daemon=True,
            ).start()
            continue
        elif method == "host.net.tcp.writeBytes":
            data = base64.b64decode(params.get("base64", ""))
            sock.sendall(data)
            result = {"size": len(data)}
        elif method == "host.net.tcp.close":
            if sock:
                sock.close()
            result = None
        elif method == "host.terminal.output":
            data = base64.b64decode(params.get("dataBase64", ""))
            output.extend(data)
            if not command_sent and (b"Last login:" in output or b"$ " in output or b"# " in output):
                input_queue.extend(b"echo FIEX_SSH_OK\n")
                command_sent = True
            result = None
        elif method == "host.terminal.input":
            data = bytes(input_queue)
            input_queue.clear()
            result = {"dataBase64": base64.b64encode(data).decode(), "eof": False}
        elif method == "host.terminal.size":
            result = {"cols": 100, "rows": 30}
        elif method == "host.terminal.closed":
            result = {"closed": finished.is_set()}
        elif method == "host.terminal.end":
            finished.set()
            result = None
        else:
            result = None

        if request_id is not None:
            send(child.stdin, send_lock, {"jsonrpc": "2.0", "id": request_id, "result": result})

        if output.count(b"FIEX_SSH_OK") >= 2:
            break

    text = output.decode("utf-8", "replace")
    print(text[-2000:])
    if output.count(b"FIEX_SSH_OK") < 2:
        stderr = child.stderr.read().decode("utf-8", "replace") if child.poll() is not None else ""
        raise AssertionError(f"SSH command output missing; stderr={stderr[:500]!r}")
    child.kill()
    print("PASS: real sshd, TCP broker, SSH handshake, PTY and command output")


if __name__ == "__main__":
    main()
