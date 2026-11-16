# Weryfikacja Rust Edition 2.1

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
