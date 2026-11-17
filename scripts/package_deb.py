"""Build a Debian/Ubuntu amd64 package from the Linux release binary."""
import argparse
import shutil
import struct
import subprocess
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
args = parser.parse_args()
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
if not args.binary.is_file():
    raise SystemExit(f'Missing binary: {args.binary}')
out = root / 'release-dist'
out.mkdir(exist_ok=True)
stage = out / 'deb-stage'
if stage.exists():
    shutil.rmtree(stage)
stage.mkdir()
stage.chmod(0o755)

def directory(relative):
    path = stage / relative
    path.mkdir(parents=True, exist_ok=True)
    return path

binary = directory('usr/bin') / 'prospekt-dj'
shutil.copy2(args.binary, binary)
binary.chmod(0o755)
docs = directory('usr/share/doc/prospekt-dj')
shutil.copy2(root / 'README.md', docs / 'README.md')
shutil.copy2(root / 'LICENSE', docs / 'copyright')
for dirname in ['docs', 'licenses']:
    shutil.copytree(root / dirname, docs / dirname)
icon = root / 'assets/icon.png'
width, height = struct.unpack('>II', icon.read_bytes()[16:24])
shutil.copy2(icon, directory(f'usr/share/icons/hicolor/{width}x{height}/apps') / 'prospekt-dj.png')
(directory('usr/share/applications') / 'prospekt-dj.desktop').write_text('''[Desktop Entry]
Type=Application
Name=Prospekt DJ BETA
Comment=Open source DJ studio — BETA
Exec=prospekt-dj
Icon=prospekt-dj
Terminal=false
Categories=AudioVideo;Audio;Mixer;
StartupNotify=true
''')
size = sum(p.stat().st_size for p in stage.rglob('*') if p.is_file())
(directory('DEBIAN') / 'control').write_text(f'''Package: prospekt-dj
Version: {version.replace("-", "~", 1)}
Section: sound
Priority: optional
Architecture: amd64
Maintainer: Prospekt DJ <prospektdj@users.noreply.github.com>
Installed-Size: {(size + 1023) // 1024}
Depends: libc6 (>= 2.35), libgcc-s1, libasound2 | libasound2t64, libx11-6, libxkbcommon0, libxkbcommon-x11-0, libgl1, xdg-utils, xdg-desktop-portal
Recommends: ffmpeg
Homepage: https://github.com/prospektdj/prospektdj
Description: BETA open source DJ studio written in Rust
 Two decks, RGB waveforms, effects, sampler, music library,
 MIDI mapping, automix and mix recording.
''')
for path in stage.rglob('*'):
    path.chmod(0o755 if path.is_dir() or path == binary else 0o644)
archive = out / f'prospekt-dj_{version}_amd64.deb'
subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(stage), str(archive)], check=True)
subprocess.run(['dpkg-deb', '--info', str(archive)], check=True)
print(archive)
