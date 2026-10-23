#!/usr/bin/env python3
"""Add a signed manifest to a .fiexplugin package and sign the catalog.

The signature covers the exact manifest.json bytes. manifest.files contains a
SHA-256 digest for every other package file.
"""
import argparse
import base64
import hashlib
import json
import os
import pathlib
import tempfile
import zipfile

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

DOMAIN = b"fiex-plugin-manifest-v1\0"
MANIFEST = "manifest.json"
SIGNATURE = "manifest.sig"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_key(path: pathlib.Path) -> Ed25519PrivateKey:
    return serialization.load_pem_private_key(path.read_bytes(), password=None)


def sign_manifest(manifest_bytes: bytes, key: Ed25519PrivateKey) -> bytes:
    return base64.b64encode(key.sign(DOMAIN + manifest_bytes))


def sign_package(path: pathlib.Path, key: Ed25519PrivateKey, publisher_id: str, publisher_name: str) -> None:
    with zipfile.ZipFile(path, "r") as source:
        entries = [(info, source.read(info.filename)) for info in source.infolist()]
    names = [info.filename for info, _ in entries]
    if len(names) != len(set(names)):
        raise SystemExit(f"{path}: duplicate ZIP entries")
    if MANIFEST not in names:
        raise SystemExit(f"{path}: manifest.json missing")
    manifest = json.loads(next(data for info, data in entries if info.filename == MANIFEST))
    public = key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    key_id = sha256(public)
    files = {}
    for info, data in entries:
        if info.is_dir() or info.filename in (MANIFEST, SIGNATURE):
            continue
        files[info.filename.replace("\\", "/")] = f"sha256:{sha256(data)}"
    manifest["publisher"] = {
        "id": publisher_id,
        "name": publisher_name,
        "keyId": key_id,
        "publicKey": base64.b64encode(public).decode(),
    }
    manifest["files"] = dict(sorted(files.items()))
    manifest_bytes = json.dumps(
        manifest, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode()
    signature = sign_manifest(manifest_bytes, key)
    handle, temporary = tempfile.mkstemp(prefix="fiex-sign-", suffix=".fiexplugin", dir=path.parent)
    os.close(handle)
    try:
        with zipfile.ZipFile(temporary, "w") as target:
            for info, data in entries:
                if info.filename in (MANIFEST, SIGNATURE) or info.is_dir():
                    continue
                copied = zipfile.ZipInfo(info.filename, info.date_time)
                copied.external_attr = info.external_attr
                copied.compress_type = info.compress_type
                target.writestr(copied, data)
            manifest_info = zipfile.ZipInfo(MANIFEST)
            manifest_info.external_attr = 0o100644 << 16
            manifest_info.compress_type = zipfile.ZIP_DEFLATED
            target.writestr(manifest_info, manifest_bytes)
            signature_info = zipfile.ZipInfo(SIGNATURE)
            signature_info.external_attr = 0o100644 << 16
            signature_info.compress_type = zipfile.ZIP_DEFLATED
            target.writestr(signature_info, signature)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    print(f"signed {path} ({key_id[:16]})")


def sign_catalog(path: pathlib.Path, key: Ed25519PrivateKey) -> None:
    # Include the exact package digests in the signed catalog.
    catalog = json.loads(path.read_text(encoding="utf-8"))
    for plugin in catalog.get("plugins", []):
        assets = plugin.get("assets") or {}
        digests = {}
        for name in assets.values():
            package = path.parent / name
            if package.is_file():
                digests[name] = sha256(package.read_bytes())
        plugin["assetSha256"] = dict(sorted(digests.items()))
    data = json.dumps(catalog, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    path.write_bytes(data + b"\n")
    signature = base64.b64encode(key.sign(b"fiex-plugin-catalog-v1\0" + data + b"\n"))
    path.with_name(path.name + ".sig").write_bytes(signature + b"\n")
    print(f"signed {path}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("packages", nargs="*", type=pathlib.Path)
    parser.add_argument("--key", required=True, type=pathlib.Path)
    parser.add_argument("--publisher-id", default="org.fiex")
    parser.add_argument("--publisher-name", default="Fiex Contributors")
    parser.add_argument("--catalog", type=pathlib.Path)
    args = parser.parse_args()
    key = load_key(args.key)
    for package in args.packages:
        sign_package(package, key, args.publisher_id, args.publisher_name)
    if args.catalog:
        sign_catalog(args.catalog, key)


if __name__ == "__main__":
    main()
