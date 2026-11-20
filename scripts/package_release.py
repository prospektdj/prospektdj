"""Package an already built native binary; uses only the Python standard library."""
import argparse
import plistlib
import shutil
import subprocess
import tarfile
import tomllib
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--target', required=True)
parser.add_argument('--platform', choices=['linux-x64', 'windows-x64', 'macos-arm64', 'macos-x64'], required=True)
args = parser.parse_args()
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
name = f'Prospekt-DJ-{version}-{args.platform}'
out = root / 'release-dist'
out.mkdir(exist_ok=True)
stage = out / name
if stage.exists():
    shutil.rmtree(stage)
stage.mkdir()
exe = 'prospekt-dj.exe' if args.platform.startswith('windows') else 'prospekt-dj'
binary = root / 'target' / args.target / 'release' / exe
if not binary.is_file():
    raise SystemExit(f'Missing compiled binary: {binary}')
if args.platform.startswith('macos'):
    app = stage / 'Prospekt DJ.app'
    contents = app / 'Contents'
    executable_dir = contents / 'MacOS'
    executable_dir.mkdir(parents=True)
    resources = contents / 'Resources'
    resources.mkdir()
    iconset = stage / 'AppIcon.iconset'
    iconset.mkdir()
    for size in [16, 32, 128, 256, 512]:
        for scale in [1, 2]:
            suffix = '@2x' if scale == 2 else ''
            subprocess.run(['sips', '-z', str(size * scale), str(size * scale), str(root / 'assets/icon.png'), '--out', str(iconset / f'icon_{size}x{size}{suffix}.png')], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(['iconutil', '-c', 'icns', str(iconset), '-o', str(resources / 'AppIcon.icns')], check=True)
    shutil.rmtree(iconset)
    with (contents / 'Info.plist').open('wb') as file:
        plistlib.dump({'CFBundleName': 'Prospekt DJ', 'CFBundleDisplayName': 'Prospekt DJ', 'CFBundleIdentifier': 'org.prospektdj.studio', 'CFBundleExecutable': exe, 'CFBundlePackageType': 'APPL', 'CFBundleIconFile': 'AppIcon', 'CFBundleShortVersionString': version, 'CFBundleVersion': version, 'LSMinimumSystemVersion': '11.0', 'NSHighResolutionCapable': True}, file)
    destination = executable_dir / exe
else:
    destination = stage / exe
shutil.copy2(binary, destination)
destination.chmod(0o755)
for filename in ['README.md', 'LICENSE']:
    shutil.copy2(root / filename, stage / filename)
for directory in ['docs', 'licenses']:
    shutil.copytree(root / directory, stage / directory)
if args.platform.startswith('macos'):
    subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(app)], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    archive = out / (name + '.zip')
    subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(stage), str(archive)], check=True)
elif args.platform.startswith('linux'):
    archive = out / (name + '.tar.gz')
    with tarfile.open(archive, 'w:gz') as file:
        file.add(stage, arcname=name)
else:
    archive = out / (name + '.zip')
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as file:
        for path in sorted(stage.rglob('*')):
            if path.is_file():
                file.write(path, path.relative_to(out))
if archive.suffix == '.zip':
    with zipfile.ZipFile(archive) as file:
        assert file.testzip() is None
else:
    with tarfile.open(archive) as file:
        assert any(member.name.endswith('/prospekt-dj') and member.mode & 0o111 for member in file.getmembers())
print(archive)
