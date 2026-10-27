#!/usr/bin/env python3
"""Local encrypted SSH protocol fixture; requires paramiko==4.0.0.

No external server, credentials, OS shell or privileged sshd is needed.
"""
import argparse
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time

import paramiko


class Server(paramiko.ServerInterface):
    def __init__(self, key, reject_pty=False):
        self.key = key
        self.reject_pty = reject_pty
        self.shell = threading.Event()

    def get_banner(self):
        return "FIEX_SSH_BANNER\r\n", "en"

    def get_allowed_auths(self, username):
        return "publickey"

    def check_auth_publickey(self, username, key):
        return paramiko.AUTH_SUCCESSFUL if key == self.key else paramiko.AUTH_FAILED

    def check_channel_request(self, kind, chanid):
        return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

    def check_channel_pty_request(self, *args):
        return not self.reject_pty

    def check_channel_shell_request(self, channel):
        self.shell.set()
        return True


def serve(listener, key, user_key, reject_pty):
    client, _ = listener.accept()
    transport = paramiko.Transport(client)
    transport.add_server_key(key)
    server = Server(user_key, reject_pty)
    try:
        transport.start_server(server=server)
        channel = transport.accept(10)
        if channel is None or not server.shell.wait(10):
            return
        # Delay until the SSH shell request has been acknowledged.
        time.sleep(0.05)
        channel.send_stderr(b"FIEX_SSH_STDERR\r\n")
        channel.send(b"fixture$ ")
        pending = b""
        channel.settimeout(10)
        while True:
            data = channel.recv(8192)
            if not data:
                break
            pending += data
            while b"\n" in pending:
                line, pending = pending.split(b"\n", 1)
                if line == b"exit":
                    channel.send_exit_status(0)
                    channel.shutdown_write()
                    return
                channel.send(line + b"\r\nFIEX_SSH_OK\r\nfixture$ ")
    finally:
        transport.close()
        listener.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    args = parser.parse_args()
    harness = str(Path(__file__).with_name("test-ssh-plugin.py"))
    host_key = paramiko.RSAKey.generate(2048)
    user_key = paramiko.RSAKey.generate(2048)
    with tempfile.TemporaryDirectory(prefix="fiex-ssh-test-") as directory:
        identity = str(Path(directory) / "identity")
        wrong_identity = str(Path(directory) / "wrong_identity")
        user_key.write_private_key_file(identity)
        paramiko.RSAKey.generate(2048).write_private_key_file(wrong_identity)
        for name, wrong_key, reject_pty, error in [
            ("shell and stderr", False, False, None),
            ("authentication failure", True, False, "SSH 认证失败"),
            ("PTY rejected", False, True, "SSH server rejected PTY"),
        ]:
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            port = listener.getsockname()[1]
            thread = threading.Thread(target=serve, args=(listener, host_key, user_key, reject_pty), daemon=True)
            thread.start()
            command = [sys.executable, harness, "--binary", args.binary, "--port", str(port),
                       "--identity", wrong_identity if wrong_key else identity,
                       "--expect-banner", "FIEX_SSH_BANNER"]
            command += ["--expect-error", error] if error else ["--expect-output", "FIEX_SSH_STDERR"]
            subprocess.run(command, check=True, timeout=50)
            thread.join(timeout=1)
            print("PASS:", name)
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
        listener.close()
        subprocess.run([sys.executable, harness, "--binary", args.binary, "--port", str(port),
                        "--identity", identity, "--expect-error", "SSH TCP"], check=True, timeout=15)


if __name__ == "__main__":
    main()
