#!/usr/bin/env python3
"""Guard the release/store contract using the actual official manifests."""
import json
from pathlib import Path
import runpy

catalog = runpy.run_path(str(Path(__file__).with_name('generate-catalog.py')))
entry = catalog['catalog_entry']
entries = {}
for path in Path('official').glob('*/manifest.json'):
    manifest = json.loads(path.read_text(encoding='utf-8'))
    value = entry(path.parent.name, manifest)
    assert value['extensions'] == manifest['extensions'], path
    assert value['assets'], path
    entries[path.parent.name] = value
assert {'ssh', 'gdrive', 'email', 'vista', 'win11'} <= entries.keys()
assert entries['ssh']['extensions']['connections']['title'] == 'SSH'
assert entries['checksum']['permissions'] == ['files.read', 'folders.read', 'metadata']
assert 'hooks' in entries['archive']['permissions']
assert len(entries['vista']['assets']) == 4
assert entries['vista']['permissions'] == []
assert set(entries['everything']['assets']) == {'windows-x86_64'}
print(f'PASS: {len(entries)} schema-2 catalog entries, permissions, contributions and platform assets')
