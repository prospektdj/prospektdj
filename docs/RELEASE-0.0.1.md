# Prospekt DJ — pre-beta 0.0.1

**PL:** Nowe wydanie łączy dotychczasowe funkcje z aktualizacjami interfejsu i audio. Nazwa produktu i paczek nie zawiera już „Rust”; kod aplikacji nadal powstaje w Rust i jest dostępny na AGPL-3.0-or-later. Numeracja została celowo zresetowana. To wydanie testowe, nie deklaracja stabilności sprzętowej.

**EN:** This release combines the existing feature set with the latest UI/audio changes. Product/package names omit “Rust”; application code remains Rust and AGPL-3.0-or-later. Version numbering was deliberately reset. This is a testing release, not hardware certification.

## Paczki / Packages

- Windows 10/11 x64 ZIP: extract and launch `prospekt-dj.exe`.
- Linux x64 tar.gz: extract and launch `./prospekt-dj`.
- Debian/Ubuntu amd64 DEB: `sudo apt install ./prospekt-dj_{{VERSION}}_amd64.deb`.
- Linux AppImage: `chmod +x Prospekt-DJ-{{VERSION}}-linux-x64.AppImage`, then run it. Without FUSE use `APPIMAGE_EXTRACT_AND_RUN=1`.
- Linux Flatpak: `flatpak install --user ./Prospekt-DJ-{{VERSION}}-linux-x64.flatpak`; launch `flatpak run org.prospektdj.ProspektDJ`.

Linux builds target Ubuntu 22.04/glibc 2.35+. Windows binaries are unsigned. No macOS package. When replacing an older, numerically higher DEB version, the package manager may request confirmation of a downgrade. Existing data is retained in the legacy configuration location.

## Nowe funkcje / New features

- Windows startup-theme fix, driver-default buffers, audio recovery/diagnostics; Windows ASIO with a separately installed compatible 64-bit driver and per-pair MASTER/PFL routing. Realtek: select WASAPI/default Speakers. No hardware driver is installed by the app.
- MIC knob and optional system microphone input mixed into MASTER/recording, excluded from PFL; square REC button.
- Black/white jog interiors, bold top BPM and lower time readouts, rotating opaque black 9 px hand with light-blue hub; saved appearance choices.
- Aligned PLAY/CUE/SYNC and SET CUE/PFL/VINYL/−/+ rows; optional half-second paused-PLAY blink; gray R/↑ controls for held Slip Reverse and confirmed eject preserving source files/Cue.
- Mouse-wheel main-waveform zoom, taller two-deck/compact four-deck waveforms, larger Cue flags; unticked minimalist mixer faders; lower Mixer tab removed.
- Live whole-catalog metadata recommendations in a bold red Polish strip, triple-arrow navigation and A/B/C/D load buttons. BPM, harmonic-key and genre ranking runs in the background; missing tags are not invented.
- BANGER BOX collections; optional 🔥 BANGER SITE/Kolekcja heading; pink ❤️ ULUBIONE, red 💿 NA DEKACH; hearts beside favorite stars; crossed music notes for tracks played this session.
- Eight colored Cue-time fields in TRACK INFO; emoji comment insertion/color preview, optional cover-scroll fades and emoji visibility (default on).
- MIDI hot-plug discovery dialog, conservative local model matching, automatic single-profile application when no controller is connected, manual variant choice and reconnect support.
- Installed Segoe UI/Arial typography with open substitutes; Microsoft fonts are not bundled.

Retained features include 2/4 decks, persistent eight-slot Hot Cue, HIGH/MID/LOW EQ and gain, filters/eight effects, sampler, PFL, A/B automix, recording, folder/library search, ratings/comments/history, artwork cache and the documented rekordbox/VirtualDJ/Mixxx/Serato import subset. The bilingual README preserves the earlier feature descriptions and importer limitations.

## Source, licenses and verification

Attached project source corresponds to the release commit. GitHub also provides source ZIP/tar.gz under this tag. Windows ASIO SDK corresponding sources are attached separately under GPLv3; notices and licenses are included with the application/source packages. Open-source fonts, graphics, mappings and libraries retain their own licenses.

`SHA256SUMS.txt` covers installers, source archives, current screenshots and screenshot-build metadata. The local optimized Linux screenshot binary is distinct from CI-built downloadable binaries. CI checks formatting, Clippy, tests, Linux/Windows compilation, archive integrity, DEB installation and AppImage/Flatpak GUI startup.

Known limits: physical Realtek/ASIO/microphone/MIDI and latency are not hardware-certified; ASIO-only microphone input is not directly captured. SYNC matches tempo without phase lock; key lock/STEMS/direct JACK are unavailable. Small four-deck windows scroll. Full EU localization and complete native/import-history compatibility remain unfinished. Detailed scope and build instructions are in [README](https://github.com/prospektdj/prospektdj/blob/{{TAG}}/README.md).
