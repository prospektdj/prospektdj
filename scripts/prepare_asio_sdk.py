"""Prepare the pinned GPLv3 SDK variant; no mutable SDK-download redirects at build time."""
import argparse
import hashlib
import io
import os
from pathlib import Path
import shutil
import urllib.request
import zipfile

URL = 'https://download.steinberg.net/sdk_downloads/ASIO-SDK_2.3.4_2025-10-15.zip'
SHA256 = 'd5ebf0c20dd2c5f43771fd0c1418f4b361bf52434ee670097cfa6b3a335e2eca'
parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--archive-output', type=Path, required=True)
parser.add_argument('--archive-input', type=Path)
args = parser.parse_args()
if args.archive_input:
    data = args.archive_input.read_bytes()
else:
    with urllib.request.urlopen(URL, timeout=90) as response:
        data = response.read(16 * 1024 * 1024)
if hashlib.sha256(data).hexdigest() != SHA256:
    raise SystemExit('ASIO SDK checksum mismatch; refusing unverified SDK sources')
args.output.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(io.BytesIO(data)) as archive:
    for info in archive.infolist():
        path = Path(info.filename)
        if len(path.parts) < 2 or path.parts[0] != 'ASIOSDK' or '..' in path.parts:
            continue
        relative = Path(*path.parts[1:])
        if relative.parts[0] not in {'common', 'host', 'LICENSE.txt', 'README.md', 'changes.txt'}:
            continue
        target = args.output / relative
        if info.is_dir():
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(info))
root = Path(__file__).resolve().parents[1]
shutil.copy2(root / 'licenses/asio-sdk/GPL-3.0.txt', args.output / 'GPL-3.0.txt')
for required in ['common/asio.h', 'common/asio.cpp', 'host/asiodrivers.cpp', 'host/pc/asiolist.cpp']:
    if not (args.output / required).is_file():
        raise SystemExit(f'Pinned SDK is missing {required}')
args.archive_output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(args.archive_output, 'w', zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(args.output.rglob('*')):
        if path.is_file():
            archive.write(path, Path('ASIOSDK') / path.relative_to(args.output))
if 'GITHUB_ENV' in os.environ:
    with open(os.environ['GITHUB_ENV'], 'a', encoding='utf-8') as file:
        file.write(f'CPAL_ASIO_DIR={args.output.resolve()}\n')
print(f'Prepared verified GPLv3 ASIO SDK at {args.output}')
print(f'Corresponding source: {args.archive_output}')
