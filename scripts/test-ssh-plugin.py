#!/usr/bin/env python3
"""Exercise a real SSH shell through the plugin's task-owned TCP broker.

Use --expect-error for authentication/connection failure regressions. The host
rejects operation-owned TCP handles, as the host does once ssh.start returns.
"""
import argparse
import base64
import json
from pathlib import Path
import queue
import socket
import subprocess
import sys
import threading
import time


# Keep standalone runs and inherited Windows test subprocesses in UTF-8 too.
for stream in (sys.stdout, sys.stderr):
    if hasattr(stream, "reconfigure"):
        stream.reconfigure(encoding="utf-8")


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
    parser.add_argument("--expect-error")
    parser.add_argument("--expect-banner")
    parser.add_argument("--expect-output")
    args = parser.parse_args()
    child = subprocess.Popen([str(Path(args.binary).resolve())], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    lock = threading.Lock()
    messages = queue.Queue()
    sock = None
    tasks = set()
    output = bytearray()
    input_queue = bytearray()
    ready = False
    terminal_error = None
    ended = False
    command_sent = False
    exit_sent = False

    def reply(request_id, result=None, error=None):
        response = {"jsonrpc": "2.0", "id": request_id}
        response["error" if error else "result"] = {"code": -32000, "message": error} if error else result
        send(child.stdin, lock, response)

    def read_messages():
        try:
            while True:
                messages.put(receive(child.stdout))
        except Exception as error:
            messages.put(error)

    def read_tcp(request_id, size, stream):
        try:
            data = stream.recv(size)
            reply(request_id, {"base64": base64.b64encode(data).decode()})
        except OSError as error:
            reply(request_id, error=str(error))

    try:
        send(child.stdin, lock, {"jsonrpc": "2.0", "id": 1, "method": "plugin.initialize",
                                 "params": {"protocolVersion": "5.0"}})
        initialized = receive(child.stdout)
        assert initialized.get("result", {}).get("pluginId") == "org.fiex.ssh", initialized
        send(child.stdin, lock, {"jsonrpc":"2.0", "id":2, "method":"connection.open",
            "params":{"connectionId":"ssh-test", "connection":{"host":args.host,"port":args.port,
            "username":args.user,"identityFile":args.identity}}})
        view = receive(child.stdout)
        assert view.get("result", {}).get("kind") == "pluginView", view
        state = view["result"]["state"]
        assert set(state) == {"sessionId"}, "credentials must never enter PluginView state"
        session_id = state["sessionId"]
        threading.Thread(target=read_messages, daemon=True).start()
        send(child.stdin, lock, {"jsonrpc":"2.0", "id":3, "method":"ssh.start",
            "params":{"sessionId":session_id,"cols":100,"rows":30}})
        started = False
        poll_id = 100
        waiting = None
        deadline = time.monotonic() + 45
        while not ended and time.monotonic() < deadline:
            if started and waiting is None:
                poll_id += 1
                waiting = poll_id
                send(child.stdin, lock, {"jsonrpc":"2.0", "id":poll_id,"method":"ssh.exchange",
                    "params":{"sessionId":session_id,"cols":100,"rows":30,
                    "dataBase64":base64.b64encode(input_queue).decode()}})
                input_queue.clear()
            try:
                message = messages.get(timeout=0.05)
            except queue.Empty:
                continue
            if isinstance(message, Exception):
                raise message
            method, request_id = message.get("method"), message.get("id")
            params = message.get("params") or {}
            if not method:
                assert not message.get("error"), message
                if request_id == 3:
                    started = True
                elif request_id == waiting:
                    waiting = None
                    result = message["result"]
                    ready = result["ready"]
                    output.extend(base64.b64decode(result["dataBase64"]))
                    ended = result["ended"]
                    terminal_error = result.get("error")
                    if ready and not command_sent and (b"$ " in output or b"# " in output):
                        input_queue.extend(b"echo FIEX_SSH_OK\n")
                        command_sent = True
                    if output.count(b"FIEX_SSH_OK") >= 2 and not exit_sent:
                        input_queue.extend(b"exit\n")
                        exit_sent = True
                continue
            task_id = params.get("authorization", {}).get("taskId")
            if method != "host.task.start":
                assert task_id in tasks, ("expired or non-task authorization", message)
            result = None
            if method == "host.task.start":
                tasks.add(params["taskId"])
            elif method == "host.task.finish":
                tasks.discard(task_id)
            elif method == "host.task.progress":
                pass
            elif method == "host.net.tcp.open":
                try:
                    sock = socket.create_connection((args.host, args.port), timeout=10)
                    result = {"stream": "ssh-integration", "peerAddress": f"{args.host}:{args.port}"}
                except OSError as error:
                    reply(request_id, error=str(error))
                    continue
            elif method == "host.net.tcp.readBytes":
                threading.Thread(target=read_tcp, args=(request_id, int(params["size"]), sock), daemon=True).start()
                continue
            elif method == "host.net.tcp.writeBytes":
                data = base64.b64decode(params["base64"])
                try:
                    sock.sendall(data)
                    result = {"size": len(data)}
                except OSError as error:
                    reply(request_id, error=str(error))
                    continue
            elif method == "host.net.tcp.close":
                sock.close()
            elif method == "host.net.tcp.shutdown":
                direction = {"read":socket.SHUT_RD,"write":socket.SHUT_WR,"both":socket.SHUT_RDWR}[params["direction"]]
                try:
                    sock.shutdown(direction)
                except OSError:
                    pass
            else:
                raise AssertionError(f"unexpected host method: {method}")
            if request_id is not None:
                reply(request_id, result)

        text = output.decode("utf-8", "replace")
        assert ended, "terminal did not end"
        if args.expect_error:
            assert args.expect_error in (terminal_error or ""), terminal_error
            assert not ready, "a failed connection was reported ready"
        else:
            assert not terminal_error, terminal_error
            assert ready and output.count(b"FIEX_SSH_OK") >= 2, text[-2000:]
        if args.expect_output:
            assert args.expect_output in text, text[-2000:]
        if args.expect_banner:
            assert args.expect_banner in text, text[-2000:]
        print("PASS: PluginView RPC, task-owned TCP, terminal readiness, output/banner and completion", terminal_error or "")
    finally:
        child.kill()
        child.wait()
        if sock:
            sock.close()


if __name__ == "__main__":
    main()
