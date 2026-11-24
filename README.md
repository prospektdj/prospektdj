# Prospekt DJ — pre-alpha 0.0.1 test

## 🇵🇱 Polski

Natywny, otwartoźródłowy program DJ dla Windows i Linux. Własny kod Rust/egui, bez Electrona. [Wydania i paczki](https://github.com/prospektdj/prospektdj/releases) · [Dokumentacja](docs/INDEX.md) · [Licencje](docs/DEPENDENCIES.md).

### 🎛️ Funkcje

- Dwa lub cztery deki, powiększone jogi czarne/białe, animowana wskazówka, pitch bend i PLAY/CUE/SYNC. Chwilowy reverse i potwierdzane wysuwanie utworu.
- Menu HOT CUE / AUTO LOOP / MANUAL LOOP / SAMPLER / EFFECTS na każdym deku. Osiem punktów CUE i osiem efektów: Filter, Echo, Reverb, Flanger, Chorus, Tremolo, Distortion, Bitcrusher. Efekty mają niezależny wybór, ON/OFF, bypass i małe pokrętła.
- Główne waveformy z siatką rytmu, dużymi flagami CUE, przybliżaniem kółkiem myszy i odbiciami między sąsiednimi kanałami. Przełączniki WAVEFORMY i BIAŁE JOGI na górnym pasku.
- BANGER SITE / Kolekcja, BANGER BOX, ulubione utwory i foldery, kolory folderów, SAMPLER, foldery systemowe, dyski i powiadomienia o nośnikach. Panele boczne zwijają się w animacji.
- Okładki bez odstępów, animacje, odbicia, menu ładowania A–D, przeciąganie prawym przyciskiem oraz okno metadanych po 6,66 s. Duży podgląd waveformu z CUE przez prawy przycisk na miniaturze.
- TRACK INFO: tonacja w kolorze, komentarze z emoji, osiem pól CUE, odsłuch poza dekami przez PHONES i przyciski ładowania utworu.
- Lokalne rekomendacje BPM/tonacja/gatunek, odstęp 1–3600 s (domyślnie 60 s), maksymalnie dwa pokazania na cykl, ostatnie 10 propozycji i statystyki. Historia TXT domyślnie włączona, retencja ponad 30 dni. Historia odtworzeń w datowanych sesjach.
- AutoMix A/B, nagrywanie miksu, mikrofon, streaming Icecast i Shoutcast v1 przez zewnętrzny FFmpeg. BCAST, MIDI LEARN, KEY i DEV na górnym pasku.
- Dwa kontrolery MIDI z osobnymi mapowaniami i wykrywaniem portów. Katalog 334 profili nie zastępuje testu konkretnego sprzętu.
- Kopiowanie, przenoszenie, zmiana nazw, podfoldery, właściwości systemowe, odświeżanie tagów i okładek. Integracja Google Drive tylko do odczytu, z własnym klientem OAuth Desktop.
- Windows: WASAPI, ASIO i wykrywanie osobno zainstalowanego ASIO4ALL. Linux: ALSA z mostem PulseAudio/PipeWire, poprawione rozpoznawanie PCM i odzyskiwanie po błędzie domyślnego wyjścia oraz wejścia.
- Dane aplikacji przenośnej są razem obok pliku wykonywalnego; komentarze i CUE w SQLite. Wyjątki pakietów systemowych, AppImage i Flatpak opisano w dokumentacji danych.

### 🚀 Uruchomienie

Rozpakuj paczkę do zapisywalnego folderu i uruchom `prospekt-dj.exe` lub `./prospekt-dj`. Nie przenoś samego pliku wykonywalnego bez danych programu. Linux wymaga ALSA, X11/OpenGL i systemowych usług audio; w kontenerze Chromebooka potrzebny jest działający most audio i pakiet `libasound2-plugins`. Opcjonalny FFmpeg jest potrzebny do nagrywania MP3 i transmisji. Nie instaluje się sterowników automatycznie.

### 🧪 Status i budowanie

To **pre-alpha 0.0.1 test**, przeznaczone do prób. Zmiana tempa zmienia wysokość dźwięku; brak key lock i STEMS. SYNC dopasowuje tempo, nie fazę. AutoMix używa A/B. Modele urządzeń wynikają z portów/profili, a nie certyfikacji USB. Linuxowe testy mostu korzystają z symulowanego PCM; Chromebook, sterowniki ASIO/Realtek, kontrolery i nośniki wymagają testów fizycznych. Próba Windows 7 jest eksperymentalna i nie oznacza potwierdzonej zgodności. Pełne tłumaczenie wszystkich ekranów na wszystkie języki nie jest ukończone.

Rust ≥1.89, `cargo build --locked --release`. [Platformy i testy](docs/BUILD.md). [Dane przenośne](docs/PORTABLE-DATA.md), [audio Chromebook](docs/CHROMEBOOK-AUDIO.md), [ASIO4ALL](docs/ASIO4ALL.md). Kod aplikacji: AGPL-3.0-or-later ([LICENSE](LICENSE)); oddzielne licencje bibliotek, mapowań, fontów i grafik zachowano w `licenses/`. Plik LICENSE zawiera oryginalny, niezmieniony tekst AGPL.

## 🇬🇧 English

Native open-source DJ application for Windows and Linux. Original Rust/egui code, without Electron. [Releases and packages](https://github.com/prospektdj/prospektdj/releases) · [Documentation](docs/INDEX.md) · [Licenses](docs/DEPENDENCIES.md).

### 🎛️ Features

- Two or four decks, enlarged black/white jogs, rotating hand, pitch bend and PLAY/CUE/SYNC. Momentary slip reverse and confirmed track ejection.
- HOT CUE / AUTO LOOP / MANUAL LOOP / SAMPLER / EFFECTS menu on each deck. Eight hot cues and eight effects: Filter, Echo, Reverb, Flanger, Chorus, Tremolo, Distortion, Bitcrusher. Effects offer independent selection, enable switches, bypass and compact knobs.
- Main waveforms with beat grid, large cue flags, wheel zoom and reflections between adjacent channels. WAVEFORMY and BIAŁE JOGI toggles in the header.
- BANGER SITE / Collection, BANGER BOX, favorite tracks/folders, folder colors, SAMPLER, system folders, disks and mounted-media notifications. Animated collapsible side panels.
- Gapless artwork gallery, transitions, reflection strips, A–D loading menus, right-button drag and metadata popup after 6.66 seconds. Right-click a track-table preview for a large waveform with cues.
- TRACK INFO: colored musical key, emoji comments, eight cue fields, off-deck headphone preview and track-loading buttons.
- Local BPM/key/genre recommendations, 1–3600-second interval (default 60), at most two presentations per cycle, ten-item history and statistics. Optional TXT history enabled by default, retention beyond 30 days. Dated playback-history sessions.
- A/B AutoMix, mix recording, microphone, Icecast and Shoutcast v1 broadcasting through external FFmpeg. BCAST, MIDI LEARN, KEY and DEV header controls.
- Two MIDI controllers with independent mapping banks and port discovery. The 334-profile catalog does not establish physical hardware certification.
- Copy, move, rename, subfolders, system properties, tag/artwork refresh. Read-only Google Drive integration requires your own Desktop OAuth client.
- Windows: WASAPI, ASIO and discovery of separately installed ASIO4ALL. Linux: ALSA with PulseAudio/PipeWire bridge, corrected PCM identification and default output/input recovery.
- Portable application data stays beside the executable; comments and cues use SQLite. System-package, AppImage and Flatpak writable-data exceptions are documented.

### 🚀 Run

Extract into a writable directory and run `prospekt-dj.exe` or `./prospekt-dj`. Keep the application data with the executable. Linux requires ALSA, X11/OpenGL and system audio services; Chromebook containers need a working audio bridge and `libasound2-plugins`. Optional external FFmpeg enables MP3 recording and broadcasting. Hardware drivers are never installed automatically.

### 🧪 Status and build

This is **pre-alpha 0.0.1 test**, intended for testing. Tempo changes affect pitch; key lock and STEMS are unavailable. SYNC matches tempo rather than beat phase. AutoMix uses A/B. Device models come from ports/profiles, not USB certification. Linux bridge tests use a simulated PCM; Chromebook, ASIO/Realtek, controllers and storage require physical testing. Windows 7 builds are experimental attempts, not verified compatibility. Complete localization of every screen into every language is unfinished.

Rust ≥1.89, `cargo build --locked --release`. See [platforms and testing](docs/BUILD.md), [portable data](docs/PORTABLE-DATA.md), [Chromebook audio](docs/CHROMEBOOK-AUDIO.md) and [ASIO4ALL](docs/ASIO4ALL.md). Application code: AGPL-3.0-or-later ([LICENSE](LICENSE)); separate dependency, mapping, font and artwork notices remain in `licenses/`. LICENSE retains the verbatim canonical AGPL text.
