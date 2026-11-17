# Prospekt DJ 2.3 BETA — current validation

- Local Linux: `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --all-targets`: **59 tests passed**. Audio/FFmpeg opt-in tests were enabled with `ALSA_CONFIG_PATH` pointing to an ALSA null fixture, `PROSPEKT_TEST_AUDIO=1` and `PROSPEKT_TEST_FFMPEG=1`; this is no physical audio-device certification.
- Native Linux/Xvfb GUI: 2/4 decks at 1920×1080 and 1280×720; also 150% X11 scaling at 1920×1080 (1280×720 logical). Verified compact 64×28 logical-pixel pads, source-time cue markers, larger animated jogs, spring pitch-bend release and larger channel/crossfader controls, vertical performance scrolling, horizontal library columns, zebra/selection, cover fixtures and the emoji name editor. A name entered with the picker was persisted verbatim as `warm up 🔥🔥`; colored Twemoji images are used for supported grapheme sequences, with a native text-edit caret plus a color name preview.
- Artwork fixtures: embedded image priority, offline cache/restart, manual assignment/rejection, metadata identity invalidation, late-result suppression, verified IDs, ambiguous results, remix mismatch, missing artist, HTTP 404/offline flow, bounded image decoding and retry/rate policy. Tests use injected transport and fixtures, without public-service availability assumptions.
- `cargo build --locked --release` builds the Linux executable. The release workflow additionally builds/tests Windows MSVC, checks DEB installation, packages AppImage/Flatpak and runs their Linux GUI smoke checks before publishing the five assets and checksums. Run/package links are available in GitHub Actions and Releases.
- Screenshots: [two decks](screenshots/v23-main.png), [four decks](screenshots/v23-four-decks.png), [1280×720](screenshots/v23-small.png), [emoji editor](screenshots/v23-emoji.png). Demo audio and original synthetic artwork fixtures are illustrative; they do not claim successful automatic matching of those invented artists.

Known limits: physical MIDI hardware, real headphone routing, actual device latency and long-duration performance remain unverified. Emoji outside the Twemoji 16.0.1 atlas use a font fallback. Conservative matching deliberately leaves uncertain artwork blank. Small/high-DPI windows require scrolling; not all four-deck controls fit simultaneously.

Previous-version validation follows for historical context.

---

# Weryfikacja Rust Edition 2.2 BETA

Aktualne kontrole 2.2 BETA: 39 testów automatycznych, cargo fmt i clippy. Szczegóły nowych testów są w sekcji Weryfikacja 2.2 BETA na końcu. Poniższe wcześniejsze sekcje zachowują historię testów wersji 2.0/2.1 BETA.

Testowane środowisko: Linux x86_64, Rust 1.100.0, natywne okno egui/eframe na Xvfb. Niezależny projekt Rust, bez uruchamiania Electrona lub przeglądarki.

## Testy wykonane

Wykonano 12 testów rdzenia z włączonym `PROSPEKT_TEST_FFMPEG=1` oraz test CPAL z `PROSPEKT_TEST_AUDIO=1` i urządzeniem ALSA null — łącznie 13 testów.

- Odtwarzanie/pauza, tempo i przeliczanie 44,1 kHz → 48 kHz; powrót do początku po końcu utworu.
- CUE/hot cue, pętla beatowa, IN/OUT/OFF.
- Ujemna prędkość scratchu, wygaśnięcie jogu, chwilowy pitch bend bez zmiany zapisanego tempa.
- Sampler bez załadowanych decków, usuwanie i przechwycenie fragmentu utworu.
- Crossfader stałej mocy, sygnał PFL, ograniczenie poziomu i zakończenie przejścia zgodnie z liczbą próbek.
- Tłumienie wysokich częstotliwości przez EQ i skończone wartości wyjścia.
- Rozpoznanie dema 120 BPM i odrzucenie ciszy.
- Trzy kodowania względnego MIDI, normalizacja Note Off, import starej mapy HTML i odrzucanie nieznanych akcji.
- Strumieniowy zapis WAV, odczyt próbek i zapis/odczyt AIFF w Rust.
- Zachowanie WAV przy błędzie nagrywania.
- Przywrócenie CUE/EQ/samplera po zmianie wyjścia.
- Eksport i ponowne dekodowanie MP3/FLAC/OGG/OPUS przez rzeczywisty FFmpeg; kontrola czasu i niezerowego sygnału.
- Rzeczywiste otwarcie strumienia CPAL, przesuwanie pozycji przez callback urządzenia i zapis dźwięku z tego callbacku.

Testowe ALSA null nie dowodzi poprawnej współpracy z fizycznym interfejsem ani mierzonej latencji. Test map MIDI nie zastępuje podłączenia kontrolera.

`cargo clippy --all-targets -- -D warnings` i formatowanie są częścią kontroli źródeł. Okno uruchomiono na Linux i obejrzano jego rzeczywisty zrzut, co ujawniło i pozwoliło poprawić szerokość miksera.

## Kompilacja Windows

Target: x86_64-pc-windows-gnu. Plik jest natywnym PE32+ GUI x64. Sprawdzono jego listę zależności DLL; nie wymaga Electrona, Node.js ani DLL MinGW obok pliku. Używa systemowych DLL Windows. W tym środowisku nie uruchamiano Windows — nie deklarujemy testu działania na tym systemie.

## Dalsze testy sprzętowe

Do wykonania na komputerze docelowym: fizyczny MIDI Learn i ponowne podłączenie, niezależne MASTER/PFL, dryf dwóch urządzeń audio, skreczowanie pod obciążeniem, routing systemowy, długie nagrania, kolekcja różnych plików M4A/MP3/FLAC. Nie zmierzono opóźnienia całego toru audio.

Sprawdzono też natywny interfejs przez X11: przełączenie jasnego motywu, otwarcie ustawień oraz uruchomienie automiksu dla trzech rzeczywistych WAV. Przy wyjściu ALSA null kolejka doszła do komunikatu „Koniec kolejki automix”. Test nie używał przeglądarki ani DOM.

Paczka Linux z tego środowiska wymaga glibc 2.39+. Workflow buduje na Ubuntu 22.04, aby umożliwić przygotowanie paczki dla starszego środowiska.

## Dodatkowa weryfikacja 2.1

Łącznie 25 testów: 13 rdzenia, 7 efektów, 4 biblioteki i 1 CPAL. Testy efektów sprawdzają bypass, czas echa, synchronizację BPM, wybrzmiewanie, stabilność parametrów oraz routing silnika. Testy katalogu obejmują wyszukiwanie, sortowanie, skrzynki, deduplikację, brakujące pliki i ponowny zapis bazy. Sprawdzono także pasma cache fali RGB.

W natywnym oknie sprawdzono zakładki DJ-FX / Samples / Mixer, przechwycenie czterech sampli, zmianę parametrów efektów, reset pokręteł, wyszukiwanie i sortowanie. Obejrzano zrzuty w rozdzielczościach 1540×1040 i 1000×760. Fizyczne urządzenia audio/MIDI oraz uruchomienie na Windows nadal wymagają sprawdzenia na docelowym komputerze.

## Weryfikacja 2.2 BETA

39 testów automatycznych: 13 rdzenia, 7 dotychczasowych efektów, 4 biblioteki, 1 CPAL (ALSA null), 8 nowych funkcji 2.2 i 6 importu/katalogu kontrolerów. Sprawdzono cztery niezależne decki, 8 hot cue na każdy deck, przywracanie czterech kanałów, ciszę kanałów C/D po przejściu do trybu dwóch decków (także ogony efektów), nowe efekty i dokładny bypass.

Kontrole katalogu obejmują 334 profile i poprawność wszystkich przypisań, wewnętrzne stałe XML, odrzucanie zewnętrznych encji, pomijanie skryptów i HID, konflikty warstw oraz względne i odwrócone kontrolki. Katalog wbudowany nie jest duplikowany w zapisywanych ustawieniach. Testy nie potwierdzają fizycznej współpracy z każdym kontrolerem; zakres częściowej konwersji jest widoczny w ustawieniach.

Font Helvetica Neue nie jest redystrybuowany. Interfejs używa wybranego lokalnego pliku fontu lub dołączonego, otwartego DejaVu Sans Bold.
