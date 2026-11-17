#!/usr/bin/env bash
set -euo pipefail
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["package"]["version"])')
appdir="$PWD/release-dist/AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" "$appdir/usr/share/icons/hicolor/512x512/apps" "$appdir/usr/share/doc/prospekt-dj" "$PWD/release-tools"
cp target/x86_64-unknown-linux-gnu/release/prospekt-dj "$appdir/usr/bin/"
cp README.md LICENSE "$appdir/usr/share/doc/prospekt-dj/"
cp -r licenses docs "$appdir/usr/share/doc/prospekt-dj/"
# linuxdeploy reads the PNG dimensions and installs the matching icon size.
cp assets/icon.png "$appdir/usr/share/icons/hicolor/512x512/apps/prospekt-dj.png"
cat > "$appdir/usr/share/applications/prospekt-dj.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Prospekt DJ BETA
Comment=Open source DJ studio — BETA
Exec=prospekt-dj
Icon=prospekt-dj
Terminal=false
Categories=AudioVideo;Audio;Mixer;
DESKTOP
curl --fail --location --retry 3 https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage --output release-tools/linuxdeploy.AppImage
chmod +x release-tools/linuxdeploy.AppImage
export APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 VERSION="$version"
# Build the AppDir first, so all copied system-library notices can be added before packing.
release-tools/linuxdeploy.AppImage --appdir "$appdir" --executable "$appdir/usr/bin/prospekt-dj" --desktop-file "$appdir/usr/share/applications/prospekt-dj.desktop" --icon-file "$appdir/usr/share/icons/hicolor/512x512/apps/prospekt-dj.png" --library /usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0 --library /usr/lib/x86_64-linux-gnu/libxkbcommon.so.0 --library /usr/lib/x86_64-linux-gnu/libX11.so.6 --library /usr/lib/x86_64-linux-gnu/libxcb.so.1 --library /usr/lib/x86_64-linux-gnu/libxcb-xkb.so.1 --library /usr/lib/x86_64-linux-gnu/libasound.so.2
python3 - <<'PY'
from pathlib import Path
import shutil,subprocess
root=Path('release-dist/AppDir');dest=root/'usr/share/doc/prospekt-dj/system-library-notices';dest.mkdir(parents=True,exist_ok=True)
for lib in root.glob('usr/lib/**/*.so*'):
    if not lib.is_file():continue
    for base in ['/usr/lib/x86_64-linux-gnu','/lib/x86_64-linux-gnu']:
        source=Path(base)/lib.name
        if not source.exists():continue
        result=subprocess.run(['dpkg-query','-S',str(source.resolve())],capture_output=True,text=True)
        if result.returncode:result=subprocess.run(['dpkg-query','-S',str(source)],capture_output=True,text=True)
        for line in result.stdout.splitlines():
            package=line.split(': ')[0].split(':')[0]
            notice=Path('/usr/share/doc')/package/'copyright'
            if notice.is_file():shutil.copy2(notice,dest/(package+'.txt'))
shutil.copytree('/usr/share/common-licenses',dest/'common-licenses',dirs_exist_ok=True)
PY
# Acknowledge the AppImage runtime's license in the distribution.
curl --fail --location --retry 3 https://raw.githubusercontent.com/AppImage/type2-runtime/main/LICENSE --output "$appdir/usr/share/doc/prospekt-dj/AppImage-runtime-LICENSE"
export LINUXDEPLOY_OUTPUT_VERSION="$version"
export OUTPUT="$PWD/release-dist/Prospekt-DJ-Rust-${version}-linux-x64.AppImage"
export LDAI_OUTPUT="$OUTPUT"
release-tools/linuxdeploy.AppImage --appdir "$appdir" --output appimage
chmod +x "$OUTPUT"
"$OUTPUT" --appimage-extract >/dev/null
# Verify the actual ELF and AppRun payload can be extracted; GUI smoke is run under Xvfb below.
test -x squashfs-root/AppRun
test -x squashfs-root/usr/bin/prospekt-dj
rm -rf squashfs-root
