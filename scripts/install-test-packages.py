#!/usr/bin/env python3
"""Install exactly the packages supported by the selected test platform."""
import argparse
import json
from pathlib import Path
import zipfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--packages', required=True, type=Path)
    parser.add_argument('--destination', required=True, type=Path)
    parser.add_argument('--platform', required=True)
    parser.add_argument('--slugs', nargs='+')
    args = parser.parse_args()
    expected = {}
    for path in Path('official').glob('*/manifest.json'):
        slug = path.parent.name
        if args.slugs and slug not in args.slugs:
            continue
        manifest = json.loads(path.read_text(encoding='utf-8'))
        entry = manifest.get('runtime', {}).get('entrypoints', {}).get(args.platform)
        if entry or manifest.get('extensions', {}).get('themes'):
            expected[f'{slug}-{args.platform}.fiexplugin'] = entry
    packages = {path.name: path for path in args.packages.glob('*.fiexplugin')
                if not args.slugs or any(path.name.startswith(slug + '-') for slug in args.slugs)}
    assert expected and packages.keys() == expected.keys(), (sorted(expected), sorted(packages))
    for name, entry in expected.items():
        destination = args.destination / packages[name].stem
        with zipfile.ZipFile(packages[name]) as archive:
            archive.extractall(destination)
        if entry:
            (destination / entry).chmod(0o755)
    print(f'Installed {len(packages)} packages for {args.platform}')


if __name__ == '__main__':
    main()
