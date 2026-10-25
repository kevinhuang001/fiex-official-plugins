#!/usr/bin/env python3
"""Create installable per-platform .fiexplugin packages."""
import argparse
import json
from pathlib import Path
import zipfile

TARGETS={"x86_64-pc-windows-msvc":"windows-x86_64",
         "aarch64-apple-darwin":"macos-aarch64",
         "x86_64-apple-darwin":"macos-x86_64",
         "x86_64-unknown-linux-gnu":"linux-x86_64"}
def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--slug",required=True)
    parser.add_argument("--target",required=True)
    parser.add_argument("--target-dir",default="target")
    parser.add_argument("--output",default="dist/plugins")
    args=parser.parse_args()
    platform=TARGETS.get(args.target)
    if not platform:parser.error("unsupported Rust target")
    root=Path("official")/args.slug
    manifest_path=root/"manifest.json"
    manifest=json.loads(manifest_path.read_text(encoding="utf-8"))
    entry=manifest.get("runtime",{}).get("entrypoints",{}).get(platform)
    theme = bool(manifest.get("extensions", {}).get("themes"))
    if not entry and not theme:
        print("Skipping",args.slug,"for",platform,"(no entrypoint)")
        return
    if entry and Path(entry).name!=entry:parser.error("invalid plugin entrypoint")
    binary=Path(args.target_dir)/args.target/"release"/entry if entry else None
    if binary and not binary.is_file():parser.error("missing compiled binary: "+str(binary))
    output=Path(args.output)
    output.mkdir(parents=True,exist_ok=True)
    archive_path=output/(args.slug+"-"+platform+".fiexplugin")
    with zipfile.ZipFile(archive_path,"w",compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
        archive.write(manifest_path,"manifest.json")
        if binary: archive.write(binary,entry)
        themes = manifest.get("extensions", {}).get("themes")
        if isinstance(themes, dict):
            themes = [themes]
        elif not isinstance(themes, list):
            themes = []
        for contribution in themes:
            archive.write(root/contribution["path"], contribution["path"])
        ui_dist=root/"ui"/"dist"
        ui=ui_dist if ui_dist.is_dir() else root/"ui"
        if ui.is_dir():
            for file in sorted(ui.rglob("*")):
                if file.is_file() and not file.is_symlink():
                    archive.write(file,file.relative_to(root).as_posix())
        # Panels may reference a source-level standalone document (for example
        # archive's ui/preview.html) while the rest of the UI is built into
        # ui/dist. Include those root documents as well, otherwise the packaged
        # manifest points at a file that does not exist.
        if ui_dist.is_dir() and (root/"ui").is_dir():
            for file in sorted((root/"ui").glob("*.html")):
                arcname=file.relative_to(root).as_posix()
                if arcname not in archive.namelist():
                    archive.write(file,arcname)
    print("Created",archive_path,archive_path.stat().st_size,"bytes")
if __name__=="__main__":main()
