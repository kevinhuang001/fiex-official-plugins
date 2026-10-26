#!/usr/bin/env python3
"""Real Git executable/provider contract test with a deterministic host broker.

Core sandbox/authorization integration is separately covered by plugin_git_e2e.
"""
import argparse
import base64
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import threading
import uuid

spec = importlib.util.spec_from_file_location("ipc", Path(__file__).with_name("test-plugin-process.py"))
ipc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ipc)


class Git:
    def __init__(self, binary, root):
        self.root = root
        self.views = {}
        self.sequence = 0
        self.child = subprocess.Popen([str(Path(binary).resolve())], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.watchdog = threading.Timer(120, self.child.kill)
        self.watchdog.start()
        self.call('plugin.initialize', {'protocolVersion': '3.0'})

    def call(self, method, params):
        self.sequence += 1
        ident = self.sequence
        ipc.send(self.child.stdin, {'jsonrpc': '2.0', 'id': ident, 'method': method, 'params': params})
        while True:
            message = ipc.receive(self.child.stdout)
            if 'method' not in message:
                assert message['id'] == ident
                return message
            p = message.get('params', {})
            if message['method'] == 'host.process.run':
                assert p['command'] == 'git'
                assert p['args'][0] == '-C' and Path(p['args'][1]).resolve() == Path(self.root)
                result = subprocess.run(['git', *p['args']], input=(p.get('stdin') or '').encode(),
                                        capture_output=True, timeout=30)
                value = {'code': result.returncode, 'stdoutBase64': base64.b64encode(result.stdout).decode(),
                         'stderrBase64': base64.b64encode(result.stderr).decode(), 'truncated': False}
            elif message['method'] == 'host.vfs.openView':
                assert p['contributionId'] == 'history'
                assert Path(p['resourceRoot']).resolve() == Path(self.root)
                assert Path(p['viewContext']['repoRoot']).resolve() == Path(self.root)
                assert len(p['viewContext']['commitId']) == 40
                value = f'fiex://mount/{uuid.uuid4().hex}/'
                self.views[value] = p['viewContext']
            else:
                raise AssertionError(message)
            ipc.send(self.child.stdin, {'jsonrpc': '2.0', 'id': message['id'], 'result': value})

    def request(self, uri, method, **params):
        return self.call(method, {'contributionId': 'history', 'viewContext': self.views[uri], 'nodeId': '', **params})

    def close(self):
        try:
            self.call('plugin.shutdown', {})
            self.child.stdin.close()
            assert self.child.wait(timeout=10) == 0
        finally:
            self.watchdog.cancel()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as workspace:
        root = str(Path(workspace).resolve())
        def git(*arguments):
            return subprocess.check_output(['git', '-C', root, *arguments], stderr=subprocess.DEVNULL).decode().strip()
        git('init')
        git('config', 'user.name', 'Fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        name = '中文 #%.bin'
        payload = bytes(range(256)) * 3
        Path(root, name).write_bytes(payload)
        git('add', '.')
        git('commit', '-m', 'A')
        commit_a = git('rev-parse', 'HEAD')
        Path(root, name).write_bytes(b'changed')
        git('add', '.')
        git('commit', '-m', 'B')
        commit_b = git('rev-parse', 'HEAD')
        plugin = Git(args.binary, root)
        try:
            a = plugin.call('git.browse', {'root': root, 'reference': commit_a})['result']
            b = plugin.call('git.browse', {'root': root, 'reference': commit_b})['result']
            assert a != b
            assert plugin.views[a]['commitId'] == commit_a and plugin.views[b]['commitId'] == commit_b
            listing = plugin.request(a, 'vfs.list', hidden=True, limit=200)['result']
            entry = listing['entries'][0]
            assert entry['name'] == name and entry['nodeId'] == name
            info = plugin.request(a, 'vfs.stat', nodeId=name)['result']
            assert info['readonly'] and info['size'] == len(payload)
            for uri, expected in [(a, payload), (b, b'changed'), (a, payload)]:
                response = plugin.request(uri, 'vfs.readSmall', nodeId=name, maxBytes=1024)['result']
                assert base64.b64decode(response['base64']) == expected
            found = plugin.request(a, 'vfs.search', query='中文', hidden=True, limit=10)['result']['entries']
            assert found[0]['relativePath'] == name
            assert 'error' in plugin.request(a, 'vfs.list', nodeId='../escape')
            assert 'error' in plugin.call('git.browse', {'root': root, 'reference': 'missing-object'})
            assert 'error' in plugin.call('fs.writeFile', {'connection': {}, 'nodeId': name})
            assert not Path(root, '.fiex').exists()
            print('PASS Git: immutable views, binary previews, opaque nodes, Unicode names and invalid contexts')
        finally:
            plugin.close()


if __name__ == '__main__':
    main()
