#!/usr/bin/env bash
set -euo pipefail
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["package"]["version"])')
app_id=org.prospektdj.ProspektDJ
build="$PWD/release-dist/flatpak-build"
repo="$PWD/release-dist/flatpak-repo"
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user --noninteractive -y flathub org.freedesktop.Platform//25.08
# The binary is already built. Using the platform as build environment avoids an unused SDK download.
flatpak build-init --arch=x86_64 "$build" "$app_id" org.freedesktop.Platform org.freedesktop.Platform 25.08
mkdir -p "$build/files/bin" "$build/files/share/applications" "$build/files/share/icons/hicolor/512x512/apps" "$build/files/share/metainfo" "$build/files/share/doc/prospekt-dj"
cp target/x86_64-unknown-linux-gnu/release/prospekt-dj "$build/files/bin/"
cp README.md LICENSE "$build/files/share/doc/prospekt-dj/"
cp -r licenses docs "$build/files/share/doc/prospekt-dj/"
cp assets/icon.png "$build/files/share/icons/hicolor/512x512/apps/$app_id.png"
cat > "$build/files/share/applications/$app_id.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Prospekt DJ PRE-BETA
Comment=Open source DJ studio — PRE-BETA
Exec=prospekt-dj
Icon=$app_id
Terminal=false
Categories=AudioVideo;Audio;Mixer;
DESKTOP
cat > "$build/files/share/metainfo/$app_id.metainfo.xml" <<META
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>$app_id</id><metadata_license>CC0-1.0</metadata_license><project_license>AGPL-3.0-or-later</project_license>
  <name>Prospekt DJ PRE-BETA</name><summary>Open source DJ studio — PRE-BETA</summary>
  <description><p>PRE-BETA DJ application with two or four decks, effects, hot cues, sampler, MIDI controller profiles and a music library.</p></description>
  <launchable type="desktop-id">$app_id.desktop</launchable><url type="homepage">https://github.com/prospektdj/prospektdj</url>
  <content_rating type="oars-1.1"/><releases><release version="$version" type="development" date="$(date -u +%F)"/></releases>
</component>
META
# Device access provides ALSA audio/MIDI. Other music folders use the file-picker portal.
flatpak build-finish --command=prospekt-dj --share=network --share=ipc --socket=x11 --socket=pulseaudio --device=all --filesystem=xdg-music "$build"
flatpak build-export --arch=x86_64 "$repo" "$build" beta
flatpak build-bundle --arch=x86_64 --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo "$repo" "release-dist/Prospekt-DJ-${version}-linux-x64.flatpak" "$app_id" beta
flatpak install --user --noninteractive -y "release-dist/Prospekt-DJ-${version}-linux-x64.flatpak"
flatpak info --user "$app_id"
